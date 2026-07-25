//! `Project`: the public API object. Batch-first; every mutating entry point
//! validates EVERY entry before mutating anything (no partial mutation).

use std::path::Path;

use crate::codec::u32_of;
use crate::consts::*;
use crate::document::{bytes_to_f32, f32_to_bytes, Document, ModelEntry, Pristine};
use crate::error::{ChituboxError, Result};
use crate::parse::parse;
use crate::stl::{all_finite, center, parse_binary_stl};
use crate::support::{build_section, decode_section};
use crate::transform::{rotate_z_bytes_in_place, rotate_z_point, rotate_z_with};
use crate::types::*;
use crate::verify::verify;
use crate::write::serialize;

/// Embedded 420-byte empty-project template.
const EMPTY_TEMPLATE: &[u8] = include_bytes!("../assets/empty.chitubox");

/// A ChituBox Basic `.chitubox` project (opaque; holds a `Document`).
#[derive(Clone, Debug)]
pub struct Project {
    doc: Document,
}

impl Project {
    // -- open / create ------------------------------------------------------

    pub fn open(path: &Path) -> Result<Project> {
        let bytes = std::fs::read(path)?;
        Self::from_bytes(bytes)
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Project> {
        Ok(Project { doc: parse(bytes)? })
    }

    pub fn new_empty() -> Project {
        Project {
            doc: parse(EMPTY_TEMPLATE.to_vec()).expect("embedded empty.chitubox must parse"),
        }
    }

    pub fn from_template_bytes(bytes: Vec<u8>) -> Result<Project> {
        Self::from_bytes(bytes)
    }

    // -- internal mutation bookkeeping -------------------------------------

    fn mark_dirty(&mut self) {
        self.doc.dirty = true;
        self.doc.original = None;
    }

    fn check_index(&self, index: usize) -> Result<()> {
        let count = self.doc.models.len();
        if index >= count {
            return Err(ChituboxError::ModelIndexOutOfRange { index, count });
        }
        Ok(())
    }

    // -- models -------------------------------------------------------------

    pub fn model_count(&self) -> usize {
        self.doc.models.len()
    }

    pub fn add_model(&mut self, model: ModelSource) -> Result<usize> {
        Ok(self.add_models(&[model])?[0])
    }

    /// Batch add. Validates and decodes ALL inputs before mutating; returns
    /// the new model indices in input order.
    pub fn add_models(&mut self, models: &[ModelSource]) -> Result<Vec<usize>> {
        // -- validate + build every entry first (no partial mutation) --
        let mut built: Vec<ModelEntry> = Vec::with_capacity(models.len());
        for m in models {
            validate_name(&m.name)?;
            for v in [m.placement.x, m.placement.y, m.placement.z_lift] {
                if !v.is_finite() {
                    return Err(ChituboxError::NonFiniteValue { context: "placement" });
                }
            }
            let centered = match &m.geometry {
                ModelGeometry::StlPath(p) => {
                    let bytes = std::fs::read(p)?;
                    parse_binary_stl(&bytes)?
                }
                ModelGeometry::Triangles(soup) => {
                    if soup.xyz.is_empty() || soup.xyz.len() % FLOATS_PER_TRI != 0 {
                        return Err(ChituboxError::BadTriangleBuffer { len: soup.xyz.len() });
                    }
                    if !all_finite(&soup.xyz) {
                        return Err(ChituboxError::NonFiniteValue { context: "triangles" });
                    }
                    center(soup.xyz.clone())?
                }
            };
            let mesh = f32_to_bytes(&centered);
            let mut raw = [0u8; MODEL_RECORD_LEN];
            let name_bytes = m.name.as_bytes();
            raw[..name_bytes.len()].copy_from_slice(name_bytes);
            // NUL terminator already present (zeroed record)
            raw[M_TRANSLATE_X..M_TRANSLATE_X + 4].copy_from_slice(&m.placement.x.to_le_bytes());
            raw[M_TRANSLATE_Y..M_TRANSLATE_Y + 4].copy_from_slice(&m.placement.y.to_le_bytes());
            raw[M_TRANSLATE_Z..M_TRANSLATE_Z + 4].copy_from_slice(&m.placement.z_lift.to_le_bytes());
            // Fail early on an impossibly-large mesh rather than truncating the
            // in-memory size field: `serialize` re-derives M_MESH_SIZE through
            // the same `u32_of` guard (write.rs), so a silent `as u32` here
            // would desync the cached record from the bytes that get written.
            raw[M_MESH_SIZE..M_MESH_SIZE + 4]
                .copy_from_slice(&u32_of(mesh.len())?.to_le_bytes());
            built.push(ModelEntry {
                raw_record: raw,
                mesh,
                support: None,
                post_filler: Vec::new(),
                pristine: None,
                session_z_rot_deg: 0.0,
            });
        }

        // -- commit --
        let first_add = self.doc.models.is_empty();
        let start = self.doc.models.len();
        self.doc.models.extend(built);
        // from-scratch builds need the native 48-byte unsupported front filler
        if first_add && self.doc.front_filler.is_empty() {
            self.doc.front_filler = vec![0u8; FROM_SCRATCH_FRONT_FILLER];
        }
        // native populated files carry settings-size 420 (empty carries 0)
        if first_add && self.doc.settings_size == 0 {
            self.doc.settings_size = SETTINGS_SIZE_CONST;
        }
        self.mark_dirty();
        Ok((start..self.doc.models.len()).collect())
    }

    pub fn models(&self) -> Vec<ModelInfo> {
        (0..self.doc.models.len())
            .map(|i| self.model_info(i))
            .collect()
    }

    pub fn model(&self, index: usize) -> Result<ModelInfo> {
        self.check_index(index)?;
        Ok(self.model_info(index))
    }

    fn model_info(&self, index: usize) -> ModelInfo {
        let me = &self.doc.models[index];
        let (min, max) = me.local_bbox();
        let t = me.translate();
        ModelInfo {
            index,
            name: me.name(),
            placement: Placement {
                x: t[0],
                y: t[1],
                z_lift: t[2],
            },
            triangle_count: me.triangle_count(),
            has_supports: me.support.is_some(),
            support_record_count: me.support.as_ref().map_or(0, |s| s.record_count()),
            local_bbox_min: min,
            local_bbox_max: max,
        }
    }

    /// Raw centered vertex stream, 9 f32 per triangle (owned copy).
    pub fn model_triangles(&self, index: usize) -> Result<Vec<f32>> {
        self.check_index(index)?;
        Ok(self.doc.models[index].mesh_f32())
    }

    // -- placement ----------------------------------------------------------

    pub fn placements(&self) -> Vec<Placement> {
        self.doc
            .models
            .iter()
            .map(|me| {
                let t = me.translate();
                Placement {
                    x: t[0],
                    y: t[1],
                    z_lift: t[2],
                }
            })
            .collect()
    }

    pub fn set_placement(&mut self, index: usize, p: Placement) -> Result<()> {
        self.set_placements(&[PlacementUpdate {
            model_index: index,
            placement: p,
        }])
    }

    pub fn set_placements(&mut self, updates: &[PlacementUpdate]) -> Result<()> {
        let count = self.doc.models.len();
        let mut seen = Vec::with_capacity(updates.len());
        for u in updates {
            if u.model_index >= count {
                return Err(ChituboxError::ModelIndexOutOfRange {
                    index: u.model_index,
                    count,
                });
            }
            if seen.contains(&u.model_index) {
                return Err(ChituboxError::DuplicateBatchIndex { index: u.model_index });
            }
            seen.push(u.model_index);
            for v in [u.placement.x, u.placement.y, u.placement.z_lift] {
                if !v.is_finite() {
                    return Err(ChituboxError::NonFiniteValue { context: "placement" });
                }
            }
        }
        for u in updates {
            self.doc.models[u.model_index]
                .set_translate([u.placement.x, u.placement.y, u.placement.z_lift]);
        }
        self.mark_dirty();
        Ok(())
    }

    // -- Z-rotation (rigid assembly) ---------------------------------------

    pub fn rotate_model_z(&mut self, index: usize, delta_deg: f32) -> Result<()> {
        self.rotate_models_z(&[ZRotation {
            model_index: index,
            delta_deg,
        }])
    }

    pub fn rotate_models_z(&mut self, rotations: &[ZRotation]) -> Result<()> {
        let count = self.doc.models.len();
        let mut seen = Vec::with_capacity(rotations.len());
        for r in rotations {
            if r.model_index >= count {
                return Err(ChituboxError::ModelIndexOutOfRange {
                    index: r.model_index,
                    count,
                });
            }
            if seen.contains(&r.model_index) {
                return Err(ChituboxError::DuplicateBatchIndex { index: r.model_index });
            }
            seen.push(r.model_index);
            if !r.delta_deg.is_finite() {
                return Err(ChituboxError::NonFiniteValue { context: "rotation" });
            }
        }
        // Two-phase to preserve the no-partial-mutation invariant: STAGE every
        // rotation purely (reading `self`, mutating nothing), which also runs
        // the post-rotation finiteness checks; only if ALL stages succeed do we
        // COMMIT. A finite coordinate near `f32::MAX` can overflow to inf/nan
        // under `x*c - y*s`, so the rebake output is re-checked against the
        // crate's finiteness barrier before it can be written.
        let mut staged = Vec::with_capacity(rotations.len());
        for r in rotations {
            staged.push(self.stage_rotation(r.model_index, r.delta_deg)?);
        }
        for st in staged {
            self.commit_rotation(st);
        }
        self.mark_dirty();
        Ok(())
    }

    /// Compute the rotated geometry for one model without mutating anything,
    /// validating that every produced coordinate is finite. Batch indices are
    /// deduplicated upstream, so per-model staging is independent.
    fn stage_rotation(&self, index: usize, delta_deg: f32) -> Result<StagedRotation> {
        let me = &self.doc.models[index];
        // Rebake source: the existing pristine snapshot, or a fresh one to
        // install on commit (taken before the FIRST rotation of this model).
        let fresh_pristine;
        let pristine: &Pristine = if let Some(p) = &me.pristine {
            fresh_pristine = None;
            p
        } else {
            let record_geo = me
                .support
                .as_ref()
                .map(|s| {
                    s.records
                        .iter()
                        .map(|r| {
                            let g = |o: usize| {
                                f32::from_le_bytes(r[o..o + 4].try_into().unwrap())
                            };
                            (
                                [g(S_START), g(S_START + 4), g(S_START + 8)],
                                [g(S_END), g(S_END + 4), g(S_END + 8)],
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            fresh_pristine = Some(Pristine {
                mesh: me.mesh.clone(),
                smesh: me.support.as_ref().map(|s| s.smesh.clone()).unwrap_or_default(),
                record_geo,
            });
            fresh_pristine.as_ref().unwrap()
        };

        let new_angle = me.session_z_rot_deg + delta_deg;
        let (s, c) = new_angle.to_radians().sin_cos();

        // rebake model mesh from pristine
        let mut mesh_f = bytes_to_f32(&pristine.mesh);
        rotate_z_with(&mut mesh_f, s, c);
        if !all_finite(&mesh_f) {
            return Err(ChituboxError::NonFiniteValue {
                context: "rotated model mesh (coordinate magnitude overflows f32)",
            });
        }
        let mesh = f32_to_bytes(&mesh_f);

        // rebake support geometry (mesh blobs + record endpoints) from pristine
        let (smesh, endpoints) = if let Some(section) = &me.support {
            // Rotate ONLY the byte ranges each record references as its mesh
            // blob. Every other smesh byte — inter-blob padding, unreferenced
            // trailing bytes, or a non-4-aligned remainder — is unknown data
            // that MUST survive verbatim (gold invariant c). Rebaking the whole
            // smesh through `bytes_to_f32` (chunks_exact(4)) would silently drop
            // a non-4-aligned tail and reinterpret dead bytes as geometry.
            let orig = section.orig_smesh_off;
            let mut new_smesh = pristine.smesh.clone();
            let smesh_len = new_smesh.len();
            for rec in &section.records {
                let abs = u32::from_le_bytes(rec[S_MESH_OFF..S_MESH_OFF + 4].try_into().unwrap());
                let len =
                    u32::from_le_bytes(rec[S_MESH_LEN..S_MESH_LEN + 4].try_into().unwrap()) as usize;
                // Parse guarantees containment; on the impossible violation
                // return an error rather than rotating endpoints while leaving
                // the blob unrotated (silent desync).
                let start = abs.checked_sub(orig).map(|v| v as usize).ok_or(
                    ChituboxError::SupportSectionMalformed {
                        detail: "record mesh_off precedes smesh start".into(),
                    },
                )?;
                let end = start.checked_add(len).ok_or(ChituboxError::ArithmeticOverflow {
                    context: "rotate blob range",
                })?;
                if end > smesh_len {
                    return Err(ChituboxError::SupportSectionMalformed {
                        detail: "record blob exceeds smesh during rotate".into(),
                    });
                }
                if !rotate_z_bytes_in_place(&mut new_smesh[start..end], s, c) {
                    return Err(ChituboxError::NonFiniteValue {
                        context: "rotated support blob (coordinate magnitude overflows f32)",
                    });
                }
            }
            let mut eps = Vec::with_capacity(section.records.len());
            for (p_start, p_end) in &pristine.record_geo {
                let ns = rotate_z_point(*p_start, s, c);
                let ne = rotate_z_point(*p_end, s, c);
                if !ns.iter().chain(ne.iter()).all(|v| v.is_finite()) {
                    return Err(ChituboxError::NonFiniteValue {
                        context: "rotated support endpoint (coordinate magnitude overflows f32)",
                    });
                }
                eps.push((ns, ne));
            }
            (Some(new_smesh), eps)
        } else {
            (None, Vec::new())
        };

        Ok(StagedRotation {
            index,
            install_pristine: fresh_pristine,
            session_z_rot_deg: new_angle,
            mesh,
            smesh,
            endpoints,
        })
    }

    /// Apply a previously-staged, finiteness-validated rotation.
    fn commit_rotation(&mut self, st: StagedRotation) {
        let me = &mut self.doc.models[st.index];
        if let Some(p) = st.install_pristine {
            me.pristine = Some(p);
        }
        me.session_z_rot_deg = st.session_z_rot_deg;
        me.mesh = st.mesh;
        if let Some(section) = me.support.as_mut() {
            if let Some(sm) = st.smesh {
                section.smesh = sm;
            }
            for (rec, (ns, ne)) in section.records.iter_mut().zip(&st.endpoints) {
                for k in 0..3 {
                    rec[S_START + k * 4..S_START + k * 4 + 4].copy_from_slice(&ns[k].to_le_bytes());
                    rec[S_END + k * 4..S_END + k * 4 + 4].copy_from_slice(&ne[k].to_le_bytes());
                }
            }
        }
    }

    pub fn transform_info(&self, index: usize) -> Result<TransformInfo> {
        self.check_index(index)?;
        let me = &self.doc.models[index];
        let (min, max) = me.local_bbox();
        Ok(TransformInfo {
            translation: me.translate(),
            session_z_rot_deg: me.session_z_rot_deg,
            rotation_scale_baked: true,
            local_bbox_min: min,
            local_bbox_max: max,
        })
    }

    // -- supports -----------------------------------------------------------

    pub fn attach_supports(&mut self, batches: &[SupportBatch]) -> Result<()> {
        let count = self.doc.models.len();
        // -- validate + build every plan before mutating --
        enum Plan {
            Remove,
            Set(crate::document::SupportSection),
        }
        let mut seen = Vec::with_capacity(batches.len());
        let mut plans: Vec<(usize, Plan, Option<f32>)> = Vec::with_capacity(batches.len());
        for b in batches {
            if b.model_index >= count {
                return Err(ChituboxError::ModelIndexOutOfRange {
                    index: b.model_index,
                    count,
                });
            }
            if seen.contains(&b.model_index) {
                return Err(ChituboxError::DuplicateBatchIndex { index: b.model_index });
            }
            seen.push(b.model_index);
            if let Some(z) = b.lift_mm {
                if !z.is_finite() {
                    return Err(ChituboxError::NonFiniteValue { context: "lift_mm" });
                }
            }

            let existing = self.doc.models[b.model_index].support.clone();
            let plan = match b.mode {
                SupportMode::Replace => {
                    if b.records.is_empty() {
                        Plan::Remove
                    } else {
                        let gh = existing
                            .as_ref()
                            .map(|s| s.group_header.clone())
                            .unwrap_or_else(|| vec![0u8; GROUP_HEADER_LEN]);
                        Plan::Set(build_section(&b.records, gh)?)
                    }
                }
                SupportMode::Append => {
                    match existing {
                        None => {
                            if b.records.is_empty() {
                                Plan::Remove // nothing to do
                            } else {
                                Plan::Set(build_section(&b.records, vec![0u8; GROUP_HEADER_LEN])?)
                            }
                        }
                        Some(mut sec) => {
                            // append new blobs + records verbatim after existing
                            // (mirrors Python write(): existing bytes untouched)
                            append_into(&mut sec, &b.records)?;
                            Plan::Set(sec)
                        }
                    }
                }
            };
            plans.push((b.model_index, plan, b.lift_mm));
        }

        // -- commit --
        for (idx, plan, lift) in plans {
            let me = &mut self.doc.models[idx];
            match plan {
                Plan::Remove => me.support = None,
                Plan::Set(sec) => me.support = Some(sec),
            }
            if let Some(z) = lift {
                let mut t = me.translate();
                t[2] = z;
                me.set_translate(t);
            }
            // Geometry changed: invalidate the rotation snapshot AND reset the
            // cumulative session angle. The freshly attached/replaced records
            // are supplied in the model's CURRENT (already-baked) frame, so the
            // post-attach mesh + records become the new pristine baseline at
            // angle 0. Failing to reset here would make the next rotate re-snap
            // the already-rotated mesh as pristine and then re-apply the old
            // cumulative angle on top of it (silently wrong geometry).
            me.pristine = None;
            me.session_z_rot_deg = 0.0;
        }
        self.mark_dirty();
        Ok(())
    }

    pub fn supports(&self, model_index: usize) -> Result<Vec<SupportRecord>> {
        self.check_index(model_index)?;
        match &self.doc.models[model_index].support {
            None => Ok(Vec::new()),
            Some(sec) => decode_section(sec),
        }
    }

    pub fn support_counts(&self) -> Vec<usize> {
        self.doc
            .models
            .iter()
            .map(|me| me.support.as_ref().map_or(0, |s| s.record_count()))
            .collect()
    }

    // -- save ---------------------------------------------------------------

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let bytes = serialize(&self.doc)?;
        verify(&bytes)?;
        // clean-save tripwire: an unmodified file must rebuild byte-identically
        if !self.doc.dirty {
            if let Some(orig) = &self.doc.original {
                if &bytes != orig {
                    return Err(ChituboxError::VerifyFailed {
                        detail: "clean structural rebuild differs from opened bytes".into(),
                    });
                }
            }
        }
        Ok(bytes)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let bytes = self.to_bytes()?;
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        // Temp name must be unique even across concurrent saves from the SAME
        // process (pid alone collides): compose pid + a high-res timestamp + a
        // process-lifetime-unique atomic sequence so two in-flight `save`s can
        // never target the same tmp path and clobber each other's bytes.
        use std::sync::atomic::{AtomicU64, Ordering};
        static TMP_SEQ: AtomicU64 = AtomicU64::new(0);
        let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp = dir.join(format!(
            ".chitubox_io.tmp.{}.{nanos}.{seq}",
            std::process::id()
        ));
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&tmp)?;
            if let Err(e) = f.write_all(&bytes).and_then(|_| f.sync_all()) {
                let _ = std::fs::remove_file(&tmp);
                return Err(e.into());
            }
        }
        if let Err(e) = std::fs::rename(&tmp, path) {
            let _ = std::fs::remove_file(&tmp);
            return Err(e.into());
        }
        Ok(())
    }

    // -- opaque / low-confidence access ------------------------------------

    pub fn parse_warnings(&self) -> Vec<ParseWarning> {
        self.doc.warnings.clone()
    }

    pub fn opaque_regions(&self) -> Vec<OpaqueRegionInfo> {
        self.opaque_locs()
            .into_iter()
            .map(|(id, kind, model_index, bytes)| OpaqueRegionInfo {
                id,
                kind: kind.to_string(),
                model_index,
                len: bytes.len(),
                nonzero_bytes: bytes.iter().filter(|&&b| b != 0).count(),
            })
            .collect()
    }

    pub fn opaque_region_bytes(&self, id: OpaqueRegionId) -> Result<Vec<u8>> {
        self.opaque_locs()
            .into_iter()
            .find(|(rid, _, _, _)| *rid == id)
            .map(|(_, _, _, bytes)| bytes)
            .ok_or(ChituboxError::UnknownOpaqueRegion(id.0))
    }

    pub fn overwrite_opaque_region_unchecked(
        &mut self,
        id: OpaqueRegionId,
        bytes: &[u8],
    ) -> Result<()> {
        let loc = self
            .resolve_opaque(id)
            .ok_or(ChituboxError::UnknownOpaqueRegion(id.0))?;
        // borrow the target Vec, length-check, replace
        let region: &mut Vec<u8> = match loc {
            OpaqueLoc::Settings => &mut self.doc.settings,
            OpaqueLoc::Preview => &mut self.doc.preview,
            OpaqueLoc::FrontFiller => &mut self.doc.front_filler,
            OpaqueLoc::GroupHeader(i) => {
                &mut self.doc.models[i].support.as_mut().unwrap().group_header
            }
            OpaqueLoc::PostFiller(i) => &mut self.doc.models[i].post_filler,
        };
        if region.len() != bytes.len() {
            return Err(ChituboxError::OpaqueLengthMismatch {
                expected: region.len(),
                got: bytes.len(),
            });
        }
        region.clear();
        region.extend_from_slice(bytes);
        self.doc.dirty = true;
        self.doc.original = None;
        Ok(())
    }

    /// Resolve an opaque id to its location. Ids are keyed on (kind,
    /// model_index) — NOT enumeration position — so a mutation that changes how
    /// many regions precede a target (e.g. removing an earlier model's
    /// supports) can never silently retarget a stale id onto a different
    /// region: the id still names its original (kind, model_index), and if that
    /// region no longer exists the resolution fails instead of aliasing.
    fn resolve_opaque(&self, id: OpaqueRegionId) -> Option<OpaqueLoc> {
        let kind = id.0 >> OPAQUE_KIND_SHIFT;
        let index = (id.0 & OPAQUE_INDEX_MASK) as usize;
        match kind {
            OPAQUE_SETTINGS if index == 0 => Some(OpaqueLoc::Settings),
            OPAQUE_PREVIEW if index == 0 => Some(OpaqueLoc::Preview),
            OPAQUE_FRONT_FILLER if index == 0 => Some(OpaqueLoc::FrontFiller),
            OPAQUE_GROUP_HEADER => self
                .doc
                .models
                .get(index)
                .filter(|m| m.support.is_some())
                .map(|_| OpaqueLoc::GroupHeader(index)),
            OPAQUE_POST_FILLER => self
                .doc
                .models
                .get(index)
                .filter(|m| !m.post_filler.is_empty())
                .map(|_| OpaqueLoc::PostFiller(index)),
            _ => None,
        }
    }

    /// Deterministic enumeration of contiguous opaque regions:
    /// settings, preview, front_filler, then per model (group_header, post_filler).
    /// Each id is the (kind, model_index) key, stable under unrelated mutations.
    fn opaque_locs(&self) -> Vec<(OpaqueRegionId, &'static str, Option<usize>, Vec<u8>)> {
        let mut out = Vec::new();
        out.push((
            encode_opaque(OPAQUE_SETTINGS, 0),
            "settings",
            None,
            self.doc.settings.clone(),
        ));
        out.push((
            encode_opaque(OPAQUE_PREVIEW, 0),
            "preview",
            None,
            self.doc.preview.clone(),
        ));
        out.push((
            encode_opaque(OPAQUE_FRONT_FILLER, 0),
            "front_filler",
            None,
            self.doc.front_filler.clone(),
        ));
        for (i, me) in self.doc.models.iter().enumerate() {
            if let Some(s) = &me.support {
                out.push((
                    encode_opaque(OPAQUE_GROUP_HEADER, i),
                    "group_header",
                    Some(i),
                    s.group_header.clone(),
                ));
            }
            if !me.post_filler.is_empty() {
                out.push((
                    encode_opaque(OPAQUE_POST_FILLER, i),
                    "gap",
                    Some(i),
                    me.post_filler.clone(),
                ));
            }
        }
        out
    }
}

// -- opaque-id encoding -----------------------------------------------------
// An OpaqueRegionId encodes (kind, model_index): the high byte is the kind
// discriminant, the low 24 bits the model index (0 for the singleton regions).
// This makes an id a stable *identity* rather than an enumeration ordinal, so a
// stale id can never silently retarget a different region after an intervening
// mutation (MAJOR 2). 24 bits caps the model index at ~16M — unreachable.
const OPAQUE_KIND_SHIFT: u32 = 24;
const OPAQUE_INDEX_MASK: u32 = (1 << OPAQUE_KIND_SHIFT) - 1;
const OPAQUE_SETTINGS: u32 = 0;
const OPAQUE_PREVIEW: u32 = 1;
const OPAQUE_FRONT_FILLER: u32 = 2;
const OPAQUE_GROUP_HEADER: u32 = 3;
const OPAQUE_POST_FILLER: u32 = 4;

fn encode_opaque(kind: u32, index: usize) -> OpaqueRegionId {
    // index is bounded by model_count, itself bounded by the u32 model table;
    // mask defensively so the kind bits are never clobbered.
    OpaqueRegionId((kind << OPAQUE_KIND_SHIFT) | (index as u32 & OPAQUE_INDEX_MASK))
}

// -- free helpers -----------------------------------------------------------

fn validate_name(name: &str) -> Result<()> {
    if name.as_bytes().contains(&0) {
        return Err(ChituboxError::NameInvalid {
            detail: "embedded NUL".into(),
        });
    }
    if name.len() > MAX_NAME_BYTES {
        return Err(ChituboxError::NameInvalid {
            detail: format!("{} bytes exceeds cap {MAX_NAME_BYTES}", name.len()),
        });
    }
    Ok(())
}

/// Append new records + blobs onto an existing section verbatim (Python
/// `write()` semantics): existing smesh/records bytes are untouched; new blobs
/// land at the end of the mesh region with absolute (orig-based) offsets.
fn append_into(
    sec: &mut crate::document::SupportSection,
    records: &[SupportRecord],
) -> Result<()> {
    let mut off = sec
        .orig_smesh_off
        .checked_add(sec.smesh.len() as u32)
        .ok_or(ChituboxError::ArithmeticOverflow { context: "append offset" })?;
    for r in records {
        // Zero-triangle record mesh rejected on the write path (mirrors
        // build_section and the model path; ChituBox semantics unprobed).
        if r.mesh.xyz.is_empty() || r.mesh.xyz.len() % FLOATS_PER_TRI != 0 {
            return Err(ChituboxError::BadTriangleBuffer { len: r.mesh.xyz.len() });
        }
        if !all_finite(&r.mesh.xyz)
            || !r.start.iter().chain(r.end.iter()).all(|v| v.is_finite())
            || !r.r_start.is_finite()
            || !r.r_end.is_finite()
        {
            return Err(ChituboxError::NonFiniteValue { context: "append record" });
        }
        let blob = f32_to_bytes(&r.mesh.xyz);
        let mesh_len = blob.len() as u32;
        let mut raw = [0u8; SEG_RECORD_LEN];
        raw[S_MAGIC..S_MAGIC + 4].copy_from_slice(&SEG_MAGIC.to_le_bytes());
        raw[S_TYPE..S_TYPE + 4].copy_from_slice(&r.segment_type.0.to_le_bytes());
        for k in 0..3 {
            raw[S_START + k * 4..S_START + k * 4 + 4].copy_from_slice(&r.start[k].to_le_bytes());
            raw[S_END + k * 4..S_END + k * 4 + 4].copy_from_slice(&r.end[k].to_le_bytes());
        }
        raw[S_R1..S_R1 + 4].copy_from_slice(&r.r_start.to_le_bytes());
        raw[S_R2..S_R2 + 4].copy_from_slice(&r.r_end.to_le_bytes());
        raw[S_MESH_OFF..S_MESH_OFF + 4].copy_from_slice(&off.to_le_bytes());
        raw[S_MESH_LEN..S_MESH_LEN + 4].copy_from_slice(&mesh_len.to_le_bytes());
        raw[S_TRAILING..S_TRAILING + 24].copy_from_slice(&r.trailing);
        sec.records.push(raw);
        sec.smesh.extend_from_slice(&blob);
        off = off
            .checked_add(mesh_len)
            .ok_or(ChituboxError::ArithmeticOverflow { context: "append offset" })?;
    }
    Ok(())
}

/// A fully-computed, finiteness-validated rotation for one model, produced by
/// `stage_rotation` and applied by `commit_rotation`. Holding owned bytes lets
/// the whole batch be validated before any model is mutated.
struct StagedRotation {
    index: usize,
    /// pristine snapshot to install (Some only on a model's first rotation)
    install_pristine: Option<Pristine>,
    session_z_rot_deg: f32,
    mesh: Vec<u8>,
    /// rebaked support mesh (None when the model has no supports)
    smesh: Option<Vec<u8>>,
    /// rotated (start, end) per support record
    endpoints: Vec<([f32; 3], [f32; 3])>,
}

/// Which contiguous opaque region an id names.
enum OpaqueLoc {
    Settings,
    Preview,
    FrontFiller,
    GroupHeader(usize),
    PostFiller(usize),
}
