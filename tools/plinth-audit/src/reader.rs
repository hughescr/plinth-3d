//! The `.chitubox` container reader.
//!
//! Navigates by the pointer chain only, never by scanning for magics
//! (`docs/specs/audit-invariants.md` §1.1). Every field of the header, the
//! model record, the support index and the 72-byte segment record is decoded,
//! **including the ones no check uses yet**, because a field nobody reads is a
//! field nobody can find a defect in.
//!
//! **Refusal policy.** A malformed file raises [`ParseError`] and the audit
//! aborts to a file-level failure. It never defaults a missing pointer, never
//! clamps an out-of-range length, and never skips a record it cannot classify.
//!
//! **Findings versus errors.** Some byte-level facts are suspicious without
//! being fatal: a nonzero byte in a span measured all-zero in every specimen,
//! a mesh region with unreferenced bytes, a trailing slot outside its measured
//! vocabulary. Those are collected in [`Findings`] and reported; they do not
//! stop the parse. `docs/specs/audit-contract.md` §1.1, §1.4 and §1.6 say
//! which is which and gives the measurement behind each.

use crate::consts as c;
use crate::vec3::{Aabb, V3};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum ParseError {
    Io(String),
    /// Any structural refusal. Maps to `FILE_PARSE_FAILED`.
    Malformed {
        offset: usize,
        what: String,
    },
    /// A record type outside the six of `audit-invariants.md` §1.1. Maps to
    /// `FILE_RECORD_SEMANTICS_UNKNOWN`: the audit cannot establish anything
    /// about a solid it cannot classify, and an unknown type is not skippable.
    UnknownRecordType {
        model: usize,
        record: usize,
        offset: usize,
        raw_type: u32,
    },
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(m) => write!(f, "io error: {m}"),
            Self::Malformed { offset, what } => {
                write!(f, "malformed container at byte {offset}: {what}")
            }
            Self::UnknownRecordType {
                model,
                record,
                offset,
                raw_type,
            } => write!(
                f,
                "model {model} record {record} at byte {offset} has unknown segment type {raw_type}"
            ),
        }
    }
}
impl std::error::Error for ParseError {}

impl ParseError {
    /// The file-level failure code this error becomes in the output document.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnknownRecordType { .. } => "FILE_RECORD_SEMANTICS_UNKNOWN",
            _ => "FILE_PARSE_FAILED",
        }
    }
}

type R<T> = Result<T, ParseError>;

// ---------------------------------------------------------------------------
// Record kinds
// ---------------------------------------------------------------------------

/// The six segment roles. **The format's own names are misleading and are not
/// used here** (`audit-invariants.md` §1.1, `audit-contract.md` §1.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RecordKind {
    /// wire 7, format name TIP: a COMPLETE contact ball. `start`/`end` are its
    /// two poles, centre is their midpoint, `r_start == r_end` is its radius.
    ContactSphere,
    /// wire 1, format name TAPER: the contact frustum, ball centre to the
    /// first structural node. Carries the declared contact depth in trailing
    /// slot 0.
    ContactTaper,
    /// wire 3, format name JOINER: THE STRUCTURAL ARM, the load-bearing
    /// member. A frustum.
    Member,
    /// wire 9, format name SHAFT: A JUNCTION BALL, not a shaft. `start`/`end`
    /// are poles at +/- r along world Z.
    JointSphere,
    /// wire 4, format name FLARE: root boss, column end to raft top.
    RootFlare,
    /// wire 6, format name PAD_OR_BRIDGE: raft. Pad and bridge share one wire
    /// type and are told apart by trailing slot 0 (`1.0` = bridge).
    RaftPadOrBridge,
}

impl RecordKind {
    #[must_use]
    pub const fn from_wire(t: u32) -> Option<Self> {
        match t {
            7 => Some(Self::ContactSphere),
            1 => Some(Self::ContactTaper),
            3 => Some(Self::Member),
            9 => Some(Self::JointSphere),
            4 => Some(Self::RootFlare),
            6 => Some(Self::RaftPadOrBridge),
            _ => None,
        }
    }
    #[must_use]
    pub const fn wire(self) -> u32 {
        match self {
            Self::ContactSphere => 7,
            Self::ContactTaper => 1,
            Self::Member => 3,
            Self::JointSphere => 9,
            Self::RootFlare => 4,
            Self::RaftPadOrBridge => 6,
        }
    }
    /// `true` for the two kinds whose connective point is the ball CENTRE, not
    /// its poles. Getting this wrong is the single most likely way to build a
    /// wrong support graph (`audit-contract.md` §1.3).
    #[must_use]
    pub const fn is_ball(self) -> bool {
        matches!(self, Self::ContactSphere | Self::JointSphere)
    }
    /// `true` for the three kinds that are right circular frusta.
    #[must_use]
    pub const fn is_frustum(self) -> bool {
        matches!(self, Self::ContactTaper | Self::Member | Self::RootFlare)
    }
    /// The short label used in failure lines and in the JSON `type` field.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ContactSphere => "tip",
            Self::ContactTaper => "taper",
            Self::Member => "member",
            Self::JointSphere => "joint",
            Self::RootFlare => "flare",
            Self::RaftPadOrBridge => "raft",
        }
    }
}

// ---------------------------------------------------------------------------
// Triangle soup
// ---------------------------------------------------------------------------

/// A raw `f32` triangle soup exactly as stored: 9 `f32` per triangle, no
/// normal, no index, no count.
///
/// The `f32` bit patterns are kept because the model mesh is welded by **exact
/// bit identity** (`audit-invariants.md` §1.4) and a round trip through `f64`
/// would make that weld a lie. [`Soup::v`] gives the `f64` view for arithmetic.
#[derive(Clone, Debug, Default)]
pub struct Soup {
    /// `tris[i] = [[x,y,z] x 3]` as stored `f32`.
    pub tris: Vec<[[f32; 3]; 3]>,
    /// Absolute byte offset the blob was read from; 0 when absent.
    pub source_offset: u32,
}

impl Soup {
    #[must_use]
    pub fn len(&self) -> usize {
        self.tris.len()
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tris.is_empty()
    }
    /// Corner `j` of triangle `i` widened to `f64`.
    #[must_use]
    pub fn v(&self, i: usize, j: usize) -> V3 {
        let t = self.tris[i][j];
        [f64::from(t[0]), f64::from(t[1]), f64::from(t[2])]
    }
    /// All three corners of triangle `i` in `f64`.
    #[must_use]
    pub fn tri(&self, i: usize) -> [V3; 3] {
        [self.v(i, 0), self.v(i, 1), self.v(i, 2)]
    }
    /// The exact `f32` bit pattern of corner `j` of triangle `i`, the key the
    /// exact weld of `audit-invariants.md` §1.4 uses.
    #[must_use]
    pub fn bits(&self, i: usize, j: usize) -> [u32; 3] {
        let t = self.tris[i][j];
        [t[0].to_bits(), t[1].to_bits(), t[2].to_bits()]
    }
    #[must_use]
    pub fn bbox(&self) -> Aabb {
        let mut b = Aabb::empty();
        for i in 0..self.len() {
            for j in 0..3 {
                b.add_point(self.v(i, j));
            }
        }
        b
    }
    /// Count of triangles whose area is EXACTLY zero. `audit-invariants.md`
    /// §3.8 makes this an exact test with no tolerance: the emitter should
    /// never write one at all.
    #[must_use]
    pub fn zero_area_triangles(&self) -> usize {
        (0..self.len())
            .filter(|&i| {
                let [a, b, cc] = self.tri(i);
                crate::vec3::tri_area(a, b, cc) == 0.0
            })
            .count()
    }
    #[must_use]
    pub fn has_nonfinite(&self) -> bool {
        (0..self.len()).any(|i| (0..3).any(|j| !crate::vec3::is_finite(self.v(i, j))))
    }
}

// ---------------------------------------------------------------------------
// Records
// ---------------------------------------------------------------------------

/// One 72-byte support segment record, every field decoded.
#[derive(Clone, Debug)]
pub struct SupportRecord {
    /// Index within this model's record array. **This is the stable id**:
    /// `contact_id` in the invariants document is the record index of a
    /// type-7 record within its model's array (`audit-invariants.md` §3).
    pub index: usize,
    /// Absolute byte offset of the record.
    pub offset: usize,
    /// Decoded magic; always `SEG_MAGIC` or the parse refuses.
    pub magic: u32,
    /// The raw wire type as stored.
    pub raw_type: u32,
    /// The role, never the format's misleading name.
    pub kind: RecordKind,
    pub start_mm: V3,
    pub end_mm: V3,
    /// +32. A RADIUS for types 1/3/4/7/9; the pad DIAMETER or bridge WIDTH for
    /// type 6 (see [`crate::consts::RAFT_FIELDS_ARE_NOT_RADII`]).
    pub r_start: f64,
    /// +36. A RADIUS for types 1/3/4/7/9; the raft HEIGHT for type 6.
    pub r_end: f64,
    pub mesh_off: u32,
    pub mesh_len: u32,
    /// The six `f32` slots of the 24 trailing bytes, at +48..+72.
    pub trailing: [f64; c::S_TRAILING_SLOTS],
    /// The trailing bytes verbatim, for anyone who wants to re-read them.
    pub trailing_raw: [u8; c::S_TRAILING_LEN],
    /// This record's own triangle soup. No two records share a blob in any
    /// file measured, so each soup is private (`audit-contract.md` §1.6).
    pub soup: Soup,
}

impl SupportRecord {
    /// The declared contact depth, mm, from trailing slot 0. Only a
    /// [`RecordKind::ContactTaper`] carries one. **DECLARED, and the field is
    /// known to lie**: every emitted taper declares 0.3 while 11.6 % of real
    /// tips reach it (`audit-contract.md` §3.1).
    #[must_use]
    pub fn declared_contact_depth_mm(&self) -> Option<f64> {
        (self.kind == RecordKind::ContactTaper).then_some(self.trailing[0])
    }
    /// `true` when a type-6 record is a bridge rather than a pad. `1.0` in
    /// trailing slot 0 means bridge (`audit-contract.md` §1.4).
    #[must_use]
    pub fn is_bridge(&self) -> Option<bool> {
        (self.kind == RecordKind::RaftPadOrBridge).then(|| self.trailing[0] != 0.0)
    }
    /// Ball centre for the two ball kinds: `midpoint(start, end)`. The poles
    /// connect to nothing.
    #[must_use]
    pub fn ball_centre(&self) -> Option<V3> {
        self.kind
            .is_ball()
            .then(|| crate::vec3::midpoint(self.start_mm, self.end_mm))
    }
    /// Distance between the two stored endpoints, mm.
    #[must_use]
    pub fn axis_length_mm(&self) -> f64 {
        crate::vec3::dist(self.start_mm, self.end_mm)
    }
}

/// The 16-byte support index a model record's +268 pointer reaches.
#[derive(Clone, Copy, Debug)]
pub struct SupportIndex {
    pub offset: usize,
    pub n_records: u32,
    pub records_off: u32,
    /// Absolute offset of this model's support-mesh region.
    pub mesh_off: u32,
    pub mesh_size: u32,
}

/// One 680-byte model record plus everything it points at.
#[derive(Clone, Debug)]
pub struct ModelEntry {
    pub index: usize,
    pub offset: usize,
    /// NUL-terminated name from +0, decoded lossily; `name_raw` keeps bytes.
    pub name: String,
    pub name_raw: Vec<u8>,
    /// +256/+260/+264. **DECLARED-ONLY**, and it cancels out of every
    /// model-relative measurement (`audit-contract.md` §1.5, §3.1).
    pub translate_mm: V3,
    pub support_index_ptr: u32,
    pub mesh_off: u32,
    pub mesh_size: u32,
    /// The model mesh blob, `mesh_size / 36` triangles, in the model's own
    /// bounding-box-centred frame -- the SAME frame the support records use,
    /// so no transform is ever needed between them.
    pub model_soup: Soup,
    pub support_index: Option<SupportIndex>,
    pub records: Vec<SupportRecord>,
}

impl ModelEntry {
    /// The plate in the centred frame: `z_plate = -translate_z`
    /// (`audit-invariants.md` §1.2).
    #[must_use]
    pub fn z_plate_mm(&self) -> f64 {
        -self.translate_mm[2]
    }
    /// Indices of this model's type-7 records, in record order. Each one is a
    /// `contact_id`.
    #[must_use]
    pub fn contact_records(&self) -> Vec<usize> {
        self.records
            .iter()
            .enumerate()
            .filter(|(_, r)| r.kind == RecordKind::ContactSphere)
            .map(|(i, _)| i)
            .collect()
    }
    #[must_use]
    pub fn count_of(&self, kind: RecordKind) -> usize {
        self.records.iter().filter(|r| r.kind == kind).count()
    }
}

// ---------------------------------------------------------------------------
// Findings
// ---------------------------------------------------------------------------

/// Byte-level facts that are suspicious without being fatal, each one measured
/// in `audit-contract.md`. They are reported, never silently dropped, and a
/// consuming check may promote any of them to a failure.
#[derive(Clone, Debug, Default)]
pub struct Findings {
    /// Nonzero bytes in `name_nul..256` or `280..680` of a model record.
    /// Measured **0** in every specimen; nonzero here is a finding, not data.
    pub model_reserved_nonzero: Vec<(usize, usize)>,
    /// Nonzero bytes in a model's 400-byte support group header. Measured
    /// **0** in every specimen.
    pub group_header_nonzero: Vec<(usize, usize)>,
    /// Records whose trailing slots 2..5 are not all zero. Zero in all 249
    /// records inspected; NOT proven zero in general.
    pub trailing_tail_nonzero: Vec<(usize, usize)>,
    /// Type-6 records whose trailing slot 1 is not the measured constant
    /// `4.0`. Its meaning is unknown; its universality is measured.
    pub raft_slot1_unexpected: Vec<(usize, usize, f64)>,
    /// Bytes of a model's support-mesh region reached by no record. Native
    /// ChituBox leaves some (morel: 3.8 %); our emitter leaves none.
    pub unreferenced_support_mesh_bytes: Vec<(usize, u64)>,
    /// Two records sharing or overlapping a mesh interval. Measured **0**
    /// everywhere; a nonzero count invalidates the "each soup is private"
    /// assumption every other check rests on.
    pub overlapping_mesh_intervals: Vec<(usize, usize, usize)>,
    /// Header field 12 outside its measured two-valued vocabulary {0, 420}.
    pub settings_size_unexpected: Option<u32>,
    /// Header field 16 not equal to the pinned 412.
    pub const16_unexpected: Option<u32>,
}

impl Findings {
    #[must_use]
    pub fn total(&self) -> usize {
        self.model_reserved_nonzero.len()
            + self.group_header_nonzero.len()
            + self.trailing_tail_nonzero.len()
            + self.raft_slot1_unexpected.len()
            + self.unreferenced_support_mesh_bytes.len()
            + self.overlapping_mesh_intervals.len()
            + usize::from(self.settings_size_unexpected.is_some())
            + usize::from(self.const16_unexpected.is_some())
    }
}

// ---------------------------------------------------------------------------
// The file
// ---------------------------------------------------------------------------

/// How the file was authored. It decides the endpoint-coincidence tolerance,
/// and it is RECONSTRUCTED from the ball tessellation, never declared
/// (`audit-contract.md` §2.1, §3.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileClass {
    /// Every ball carries 224 triangles: our own emitter.
    Emitted,
    /// Ball tessellation is anything else, including native ChituBox's 480.
    Native,
    /// No ball records at all, so the tessellation says nothing.
    Undetermined,
}

impl FileClass {
    /// The endpoint-coincidence tolerance for this class. **Do not use one
    /// constant for both cases without saying which file class it is for.**
    #[must_use]
    pub const fn join_tol_mm(self) -> f64 {
        match self {
            Self::Emitted => c::JOIN_TOL_EMITTED_MM,
            // Undetermined gets the looser tolerance because guessing tight on
            // an unclassified file silently drops real joins.
            Self::Native | Self::Undetermined => c::JOIN_TOL_NATIVE_MM,
        }
    }
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Emitted => "emitted",
            Self::Native => "native",
            Self::Undetermined => "undetermined",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ChituboxFile {
    pub path: PathBuf,
    pub byte_len: usize,
    pub sha256: String,
    pub model_count: u32,
    pub model_table_off: u32,
    pub settings_size: u32,
    pub const16: u32,
    pub support_anchor_off: u32,
    pub models: Vec<ModelEntry>,
    pub findings: Findings,
    pub file_class: FileClass,
    /// Ball triangle counts observed, for the classification evidence.
    pub ball_triangle_counts: BTreeMap<usize, usize>,
}

impl ChituboxFile {
    /// Read and fully decode a `.chitubox` file.
    pub fn open(path: &Path) -> R<Self> {
        let bytes =
            std::fs::read(path).map_err(|e| ParseError::Io(format!("{}: {e}", path.display())))?;
        Self::parse(path, &bytes)
    }

    /// Decode from bytes already in hand (the fixture harness uses this).
    pub fn parse(path: &Path, b: &[u8]) -> R<Self> {
        if b.len() < c::HEADER_LEN {
            return Err(ParseError::Malformed {
                offset: 0,
                what: format!("file is {} bytes, shorter than the 24-byte header", b.len()),
            });
        }
        let magic = rd_u32(b, c::H_MAGIC)?;
        if magic != c::FILE_MAGIC {
            return Err(ParseError::Malformed {
                offset: c::H_MAGIC,
                what: format!("file magic {magic:#010x}, expected {:#010x}", c::FILE_MAGIC),
            });
        }
        let model_count = rd_u32(b, c::H_MODEL_COUNT)?;
        let model_table_off = rd_u32(b, c::H_OFF8)?;
        let settings_size = rd_u32(b, c::H_SETTINGS_SIZE)?;
        let const16 = rd_u32(b, c::H_CONST16)?;
        let support_anchor_off = rd_u32(b, c::H_OFF20)?;

        let mut findings = Findings::default();
        if settings_size != 0 && settings_size != c::SETTINGS_SIZE_POPULATED {
            findings.settings_size_unexpected = Some(settings_size);
        }
        if const16 != c::CONST16 {
            findings.const16_unexpected = Some(const16);
        }

        // The one structural identity the header must satisfy.
        let expected_anchor = u64::from(model_table_off)
            + u64::from(c::MODEL_RECORD_LEN as u32) * u64::from(model_count);
        if support_anchor_off != 0 && u64::from(support_anchor_off) != expected_anchor {
            return Err(ParseError::Malformed {
                offset: c::H_OFF20,
                what: format!(
                    "support anchor {support_anchor_off} != off8 + 680*model_count \
                     ({expected_anchor})"
                ),
            });
        }

        let mut models = Vec::with_capacity(model_count as usize);
        for mi in 0..model_count as usize {
            let moff = model_table_off as usize + mi * c::MODEL_RECORD_LEN;
            models.push(parse_model(b, mi, moff, &mut findings)?);
        }

        // Ball tessellation census -> file class.
        let mut ball_triangle_counts: BTreeMap<usize, usize> = BTreeMap::new();
        for m in &models {
            for r in &m.records {
                if r.kind.is_ball() {
                    *ball_triangle_counts.entry(r.soup.len()).or_default() += 1;
                }
            }
        }
        let file_class = if ball_triangle_counts.is_empty() {
            FileClass::Undetermined
        } else if ball_triangle_counts.len() == 1
            && ball_triangle_counts.contains_key(&c::EMITTED_SPHERE_TRIANGLES)
        {
            FileClass::Emitted
        } else {
            FileClass::Native
        };

        Ok(Self {
            path: path.to_path_buf(),
            byte_len: b.len(),
            sha256: crate::sha256::hex(b),
            model_count,
            model_table_off,
            settings_size,
            const16,
            support_anchor_off,
            models,
            findings,
            file_class,
            ball_triangle_counts,
        })
    }
}

fn parse_model(b: &[u8], mi: usize, moff: usize, f: &mut Findings) -> R<ModelEntry> {
    if moff + c::MODEL_RECORD_LEN > b.len() {
        return Err(ParseError::Malformed {
            offset: moff,
            what: format!("model record {mi} runs past the end of the file"),
        });
    }
    let rec = &b[moff..moff + c::MODEL_RECORD_LEN];
    let nul = rec[..c::M_NAME_FIELD_LEN]
        .iter()
        .position(|&x| x == 0)
        .unwrap_or(c::M_NAME_FIELD_LEN);
    let name_raw = rec[..nul].to_vec();
    let name = String::from_utf8_lossy(&name_raw).into_owned();

    // Measured all-zero spans; a nonzero byte is a finding, not ordinary data.
    let reserved_nonzero = rec[nul..c::M_NAME_FIELD_LEN]
        .iter()
        .filter(|&&x| x != 0)
        .count()
        + rec[c::M_UNMAPPED_TAIL..]
            .iter()
            .filter(|&&x| x != 0)
            .count();
    if reserved_nonzero > 0 {
        f.model_reserved_nonzero.push((mi, reserved_nonzero));
    }

    let translate_mm = [
        f64::from(rd_f32(b, moff + c::M_TRANSLATE_X)?),
        f64::from(rd_f32(b, moff + c::M_TRANSLATE_Y)?),
        f64::from(rd_f32(b, moff + c::M_TRANSLATE_Z)?),
    ];
    let support_index_ptr = rd_u32(b, moff + c::M_SUPPORT_INDEX_PTR)?;
    let mesh_off = rd_u32(b, moff + c::M_MESH_OFF)?;
    let mesh_size = rd_u32(b, moff + c::M_MESH_SIZE)?;

    let model_soup = read_soup(b, mesh_off, mesh_size, &format!("model {mi} mesh"))?;

    let mut support_index = None;
    let mut records = Vec::new();
    if support_index_ptr != 0 {
        let sio = support_index_ptr as usize;
        if sio + c::SUPPORT_INDEX_LEN > b.len() {
            return Err(ParseError::Malformed {
                offset: sio,
                what: format!("model {mi} support index runs past the end of the file"),
            });
        }
        let idx = SupportIndex {
            offset: sio,
            n_records: rd_u32(b, sio + c::SI_N_RECORDS)?,
            records_off: rd_u32(b, sio + c::SI_RECORDS_OFF)?,
            mesh_off: rd_u32(b, sio + c::SI_MESH_OFF)?,
            mesh_size: rd_u32(b, sio + c::SI_MESH_SIZE)?,
        };
        // Measured invariant (`audit-contract.md` §1.1, §1.6): the support-mesh
        // region ends exactly where the 16-byte index begins. The contract
        // writes this as `mesh_off + mesh_size == index_off`; the records then
        // follow the index and its 400-byte group header, so
        // `records_off == index_off + 16 + 400`. Verified against
        // `exports/sphere-first-artifact.chitubox`: 894208 + 416 = 894624.
        let end = u64::from(idx.mesh_off) + u64::from(idx.mesh_size);
        if end != sio as u64 {
            return Err(ParseError::Malformed {
                offset: sio,
                what: format!(
                    "model {mi} support index: mesh_off+mesh_size ({end}) != index offset ({sio})"
                ),
            });
        }
        let expected_records_off =
            sio as u64 + c::SUPPORT_INDEX_LEN as u64 + c::GROUP_HEADER_LEN as u64;
        if u64::from(idx.records_off) != expected_records_off {
            return Err(ParseError::Malformed {
                offset: sio + c::SI_RECORDS_OFF,
                what: format!(
                    "model {mi} support index: records_off ({}) != index + 16 + 400 \
                     ({expected_records_off})",
                    idx.records_off
                ),
            });
        }
        // The 400-byte group header sits between the index end and records_off
        // ONLY when the layout leaves room for it; measure whatever is there.
        let gh_start = sio + c::SUPPORT_INDEX_LEN;
        if gh_start + c::GROUP_HEADER_LEN <= b.len() {
            let nz = b[gh_start..gh_start + c::GROUP_HEADER_LEN]
                .iter()
                .filter(|&&x| x != 0)
                .count();
            if nz > 0 {
                f.group_header_nonzero.push((mi, nz));
            }
        }

        records.reserve(idx.n_records as usize);
        for ri in 0..idx.n_records as usize {
            let roff = idx.records_off as usize + ri * c::SEG_RECORD_LEN;
            records.push(parse_record(b, mi, ri, roff, f)?);
        }

        // Mesh-region coverage (audit-contract.md §1.6): every record's
        // interval must sit inside the region, must not overlap another, and
        // the unreferenced remainder is reported.
        let region_lo = u64::from(idx.mesh_off);
        let region_hi = region_lo + u64::from(idx.mesh_size);
        let mut ivs: Vec<(u64, u64, usize)> = records
            .iter()
            .filter(|r| r.mesh_len > 0)
            .map(|r| {
                (
                    u64::from(r.mesh_off),
                    u64::from(r.mesh_off) + u64::from(r.mesh_len),
                    r.index,
                )
            })
            .collect();
        ivs.sort_unstable();
        let mut covered = 0u64;
        for w in ivs.windows(2) {
            if w[1].0 < w[0].1 {
                f.overlapping_mesh_intervals.push((mi, w[0].2, w[1].2));
            }
        }
        for &(lo, hi, ri) in &ivs {
            if lo < region_lo || hi > region_hi {
                return Err(ParseError::Malformed {
                    offset: lo as usize,
                    what: format!(
                        "model {mi} record {ri} mesh interval [{lo},{hi}) escapes the \
                         support-mesh region [{region_lo},{region_hi})"
                    ),
                });
            }
            covered += hi - lo;
        }
        let region_len = u64::from(idx.mesh_size);
        if covered < region_len {
            f.unreferenced_support_mesh_bytes
                .push((mi, region_len - covered));
        }
        support_index = Some(idx);
    }

    Ok(ModelEntry {
        index: mi,
        offset: moff,
        name,
        name_raw,
        translate_mm,
        support_index_ptr,
        mesh_off,
        mesh_size,
        model_soup,
        support_index,
        records,
    })
}

fn parse_record(b: &[u8], mi: usize, ri: usize, off: usize, f: &mut Findings) -> R<SupportRecord> {
    if off + c::SEG_RECORD_LEN > b.len() {
        return Err(ParseError::Malformed {
            offset: off,
            what: format!("model {mi} record {ri} runs past the end of the file"),
        });
    }
    let magic = rd_u32(b, off + c::S_MAGIC)?;
    if magic != c::SEG_MAGIC {
        return Err(ParseError::Malformed {
            offset: off + c::S_MAGIC,
            what: format!(
                "model {mi} record {ri} magic {magic:#010x}, expected {:#010x}",
                c::SEG_MAGIC
            ),
        });
    }
    let raw_type = rd_u32(b, off + c::S_TYPE)?;
    let Some(kind) = RecordKind::from_wire(raw_type) else {
        return Err(ParseError::UnknownRecordType {
            model: mi,
            record: ri,
            offset: off + c::S_TYPE,
            raw_type,
        });
    };
    let start_mm = rd_v3(b, off + c::S_START)?;
    let end_mm = rd_v3(b, off + c::S_END)?;
    let r_start = f64::from(rd_f32(b, off + c::S_R1)?);
    let r_end = f64::from(rd_f32(b, off + c::S_R2)?);
    let mesh_off = rd_u32(b, off + c::S_MESH_OFF)?;
    let mesh_len = rd_u32(b, off + c::S_MESH_LEN)?;

    let mut trailing = [0.0f64; c::S_TRAILING_SLOTS];
    for (s, slot) in trailing.iter_mut().enumerate() {
        *slot = f64::from(rd_f32(b, off + c::S_TRAILING + s * 4)?);
    }
    let mut trailing_raw = [0u8; c::S_TRAILING_LEN];
    trailing_raw.copy_from_slice(&b[off + c::S_TRAILING..off + c::S_TRAILING + c::S_TRAILING_LEN]);

    if trailing[2..].iter().any(|&v| v != 0.0) {
        f.trailing_tail_nonzero.push((mi, ri));
    }
    if kind == RecordKind::RaftPadOrBridge && trailing[1] != 4.0 {
        f.raft_slot1_unexpected.push((mi, ri, trailing[1]));
    }

    let soup = read_soup(
        b,
        mesh_off,
        mesh_len,
        &format!("model {mi} record {ri} mesh"),
    )?;

    Ok(SupportRecord {
        index: ri,
        offset: off,
        magic,
        raw_type,
        kind,
        start_mm,
        end_mm,
        r_start,
        r_end,
        mesh_off,
        mesh_len,
        trailing,
        trailing_raw,
        soup,
    })
}

fn read_soup(b: &[u8], off: u32, len: u32, what: &str) -> R<Soup> {
    if len == 0 {
        return Ok(Soup {
            tris: Vec::new(),
            source_offset: off,
        });
    }
    let o = off as usize;
    let l = len as usize;
    if !l.is_multiple_of(c::MESH_BYTES_PER_TRI) {
        return Err(ParseError::Malformed {
            offset: o,
            what: format!("{what}: length {l} is not a multiple of 36"),
        });
    }
    if o.checked_add(l).is_none_or(|end| end > b.len()) {
        return Err(ParseError::Malformed {
            offset: o,
            what: format!("{what}: [{o},{}) runs past the end of the file", o + l),
        });
    }
    let n = l / c::MESH_BYTES_PER_TRI;
    let mut tris = Vec::with_capacity(n);
    for t in 0..n {
        let base = o + t * c::MESH_BYTES_PER_TRI;
        let mut tri = [[0.0f32; 3]; 3];
        for (vi, v) in tri.iter_mut().enumerate() {
            for (ci, comp) in v.iter_mut().enumerate() {
                let p = base + (vi * 3 + ci) * 4;
                *comp = f32::from_le_bytes([b[p], b[p + 1], b[p + 2], b[p + 3]]);
            }
        }
        tris.push(tri);
    }
    Ok(Soup {
        tris,
        source_offset: off,
    })
}

// ---------------------------------------------------------------------------
// Bounds-checked primitive reads. Never default, never clamp.
// ---------------------------------------------------------------------------

fn rd_u32(b: &[u8], off: usize) -> R<u32> {
    if off + 4 > b.len() {
        return Err(ParseError::Malformed {
            offset: off,
            what: "u32 read past the end of the file".to_string(),
        });
    }
    Ok(u32::from_le_bytes([
        b[off],
        b[off + 1],
        b[off + 2],
        b[off + 3],
    ]))
}

fn rd_f32(b: &[u8], off: usize) -> R<f32> {
    if off + 4 > b.len() {
        return Err(ParseError::Malformed {
            offset: off,
            what: "f32 read past the end of the file".to_string(),
        });
    }
    Ok(f32::from_le_bytes([
        b[off],
        b[off + 1],
        b[off + 2],
        b[off + 3],
    ]))
}

fn rd_v3(b: &[u8], off: usize) -> R<V3> {
    Ok([
        f64::from(rd_f32(b, off)?),
        f64::from(rd_f32(b, off + 4)?),
        f64::from(rd_f32(b, off + 8)?),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_file_is_refused_not_defaulted() {
        let e = ChituboxFile::parse(Path::new("x"), &[0u8; 8]).unwrap_err();
        assert_eq!(e.code(), "FILE_PARSE_FAILED");
    }

    #[test]
    fn a_bad_magic_is_refused() {
        let mut b = vec![0u8; 24];
        b[..4].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
        let e = ChituboxFile::parse(Path::new("x"), &b).unwrap_err();
        assert!(matches!(e, ParseError::Malformed { .. }));
    }

    #[test]
    fn record_kinds_round_trip_through_the_wire() {
        for k in [
            RecordKind::ContactSphere,
            RecordKind::ContactTaper,
            RecordKind::Member,
            RecordKind::JointSphere,
            RecordKind::RootFlare,
            RecordKind::RaftPadOrBridge,
        ] {
            assert_eq!(RecordKind::from_wire(k.wire()), Some(k));
        }
        // The types the format does not define must not decode to anything.
        for t in [0u32, 2, 5, 8, 10, 99] {
            assert_eq!(RecordKind::from_wire(t), None, "type {t}");
        }
    }
}
