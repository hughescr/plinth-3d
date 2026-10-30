//! The writer. It checks the whole project first, computes the layout with checked
//! arithmetic, and only then produces bytes. Every offset, count and length in the
//! file comes from the layout, never from stored values.

use crate::error::WriteError;
use crate::mesh;
use crate::preview::Preview;
use crate::project::{
    DESCRIPTOR_BYTES, HEADER_BYTES, MAGIC, MODEL_RECORD_BYTES, Model, NAME_BYTES,
    PREVIEW_DATA_OFFSET, PREVIEW_HEADER_BYTES, PREVIEW_HEADER_OFFSET, Project, ZERO_BLOCK_BYTES,
};
use crate::supports::{
    GROUP_HEADER_BYTES, INDEX_BYTES, PRESET_ENTRY_BYTES, RECORD_BYTES, RECORD_MAGIC, Supports,
    TRAILER_BYTES,
};

/// Positions of one supported model's support section.
struct SupPlan {
    blob_off: u64,
    blob_size: u64,
    index_off: u64,
    records_off: u64,
}

struct ModelPlan {
    mesh_off: u64,
    mesh_size: u64,
    sup: Option<SupPlan>,
}

struct BlockPlan {
    table: u64,
    attr: u64,
}

struct Layout {
    off8: u64,
    off20: u64,
    total: u64,
    blocks: Vec<Option<BlockPlan>>,
    models: Vec<ModelPlan>,
}

fn add(a: u64, b: u64, what: &'static str) -> Result<u64, WriteError> {
    a.checked_add(b).ok_or(WriteError::TooLarge { what })
}

fn mul(a: usize, b: u64, what: &'static str) -> Result<u64, WriteError> {
    (a as u64)
        .checked_mul(b)
        .ok_or(WriteError::TooLarge { what })
}

fn w32(v: u64, what: &'static str) -> Result<u32, WriteError> {
    u32::try_from(v).map_err(|_| WriteError::TooLarge { what })
}

impl Project {
    /// Serialise to the `.chitubox` byte layout. Every offset, count and length is
    /// rebuilt from the typed data.
    ///
    /// # Errors
    /// Returns [`WriteError`] (before producing any byte) for a bad name, a
    /// non-finite number, a preview whose pixel count is not width times height, or a
    /// size or offset that does not fit its 32-bit field.
    pub fn to_bytes(&self) -> Result<Vec<u8>, WriteError> {
        self.validate()?;
        let preview = self.preview.as_ref().map(Preview::encode).transpose()?;
        let layout = self.layout(preview.as_deref().map(<[u8]>::len))?;
        let mut out = Vec::with_capacity(usize::try_from(layout.total).unwrap_or(0));
        self.write_head(&mut out, &layout, preview.as_deref())?;
        self.write_front(&mut out, &layout)?;
        for (m, plan) in self.models.iter().zip(&layout.models) {
            write_model_data(&mut out, m, plan)?;
        }
        debug_assert_eq!(out.len() as u64, layout.total);
        Ok(out)
    }

    fn validate(&self) -> Result<(), WriteError> {
        if let Some(p) = &self.preview {
            p.validate()?;
        }
        for (i, m) in self.models.iter().enumerate() {
            let name_ok = m.name.len() < NAME_BYTES && !m.name.as_bytes().contains(&0);
            if !name_ok {
                return Err(WriteError::BadName { model: i });
            }
            let finite = m.translate.iter().all(|v| v.is_finite()) && mesh::all_finite(&m.mesh);
            if !finite {
                return Err(WriteError::NonFinite { what: "model" });
            }
            if let Some(s) = &m.supports {
                validate_supports(s)?;
            }
        }
        Ok(())
    }

    fn layout(&self, preview_len: Option<usize>) -> Result<Layout, WriteError> {
        let n = self.models.len();
        let mut pos = PREVIEW_HEADER_OFFSET;
        if let Some(len) = preview_len {
            pos = add(pos, PREVIEW_HEADER_BYTES + len as u64, "preview")?;
        }
        let off8 = pos;
        pos = add(
            pos,
            mul(n, MODEL_RECORD_BYTES, "model table")?,
            "model table",
        )?;
        let off20 = if n == 0 { 0 } else { pos };
        pos = add(pos, mul(n, DESCRIPTOR_BYTES, "descriptors")?, "descriptors")?;
        let mut blocks = Vec::with_capacity(n);
        for m in &self.models {
            let (plan, next) = plan_block(m.supports.as_ref(), pos)?;
            blocks.push(plan);
            pos = next;
        }
        let mut models = Vec::with_capacity(n);
        for m in &self.models {
            let (plan, next) = plan_model(m, pos)?;
            models.push(plan);
            pos = next;
        }
        w32(pos, "file size")?;
        Ok(Layout {
            off8,
            off20,
            total: pos,
            blocks,
            models,
        })
    }

    fn write_head(
        &self,
        out: &mut Vec<u8>,
        l: &Layout,
        preview: Option<&[u8]>,
    ) -> Result<(), WriteError> {
        put32(out, MAGIC);
        put32(out, w32(self.models.len() as u64, "model count")?);
        put32(out, w32(l.off8, "model table offset")?);
        put32(
            out,
            if preview.is_some() {
                w32(PREVIEW_HEADER_OFFSET, "f12")?
            } else {
                0
            },
        );
        put32(out, self.header_f16);
        put32(out, w32(l.off20, "front block offset")?);
        debug_assert_eq!(out.len() as u64, HEADER_BYTES);
        zeros(out, ZERO_BLOCK_BYTES);
        if let (Some(p), Some(data)) = (&self.preview, preview) {
            put32(out, p.width);
            put32(out, p.height);
            put32(out, w32(PREVIEW_DATA_OFFSET, "preview data offset")?);
            put32(out, w32(data.len() as u64, "preview data length")?);
            zeros(out, 16);
            out.extend_from_slice(data);
        }
        for (m, plan) in self.models.iter().zip(&l.models) {
            write_model_record(out, m, plan)?;
        }
        Ok(())
    }

    fn write_front(&self, out: &mut Vec<u8>, l: &Layout) -> Result<(), WriteError> {
        for (m, block) in self.models.iter().zip(&l.blocks) {
            match (&m.supports, block) {
                (Some(s), Some(b)) => {
                    put64(out, s.presets.len() as u64);
                    put64(out, b.table);
                    put64(out, b.attr);
                }
                _ => zeros(out, DESCRIPTOR_BYTES),
            }
        }
        for (m, block) in self.models.iter().zip(&l.blocks) {
            if let (Some(s), Some(b)) = (&m.supports, block) {
                write_block(out, s, b.table)?;
            }
        }
        Ok(())
    }
}

fn validate_supports(s: &Supports) -> Result<(), WriteError> {
    for r in &s.records {
        let finite = r.start.iter().chain(&r.end).all(|v| v.is_finite())
            && r.r1.is_finite()
            && r.r2.is_finite()
            && mesh::all_finite(&r.mesh);
        if !finite {
            return Err(WriteError::NonFinite {
                what: "support record",
            });
        }
    }
    Ok(())
}

/// Plan the preset block of a supported model starting at `pos`; returns the next position.
fn plan_block(s: Option<&Supports>, pos: u64) -> Result<(Option<BlockPlan>, u64), WriteError> {
    let Some(s) = s else {
        return Ok((None, pos));
    };
    let names: u64 = s.presets.iter().map(|p| p.name.len() as u64).sum();
    let attr = add(
        add(
            pos,
            mul(s.presets.len(), PRESET_ENTRY_BYTES, "preset table")?,
            "preset table",
        )?,
        names,
        "preset names",
    )?;
    let end = add(attr, mul(s.attrs.len(), 8, "attr array")?, "attr array")?;
    Ok((Some(BlockPlan { table: pos, attr }), end))
}

/// Plan a model's mesh and support section starting at `pos`.
fn plan_model(m: &Model, pos: u64) -> Result<(ModelPlan, u64), WriteError> {
    let mesh_size = mesh::byte_len(&m.mesh, "model mesh")?;
    let blob_off = add(pos, mesh_size, "model mesh")?;
    let Some(s) = &m.supports else {
        return Ok((
            ModelPlan {
                mesh_off: pos,
                mesh_size,
                sup: None,
            },
            blob_off,
        ));
    };
    let mut blob_size = s.unreferenced_blob.len() as u64;
    for r in &s.records {
        blob_size = add(
            blob_size,
            mesh::byte_len(&r.mesh, "record mesh")?,
            "support mesh blob",
        )?;
    }
    let index_off = add(blob_off, blob_size, "support index")?;
    let records_off = add(
        add(index_off, INDEX_BYTES, "support index")?,
        GROUP_HEADER_BYTES,
        "group header",
    )?;
    let end = add(
        add(
            records_off,
            mul(s.records.len(), RECORD_BYTES, "support records")?,
            "support records",
        )?,
        TRAILER_BYTES,
        "trailer",
    )?;
    let sup = SupPlan {
        blob_off,
        blob_size,
        index_off,
        records_off,
    };
    Ok((
        ModelPlan {
            mesh_off: pos,
            mesh_size,
            sup: Some(sup),
        },
        end,
    ))
}

fn write_model_record(out: &mut Vec<u8>, m: &Model, plan: &ModelPlan) -> Result<(), WriteError> {
    let start = out.len();
    out.extend_from_slice(m.name.as_bytes());
    out.resize(start + NAME_BYTES, 0);
    for v in m.translate {
        out.extend_from_slice(&v.to_le_bytes());
    }
    put32(
        out,
        w32(
            plan.sup.as_ref().map_or(0, |s| s.index_off),
            "support index offset",
        )?,
    );
    put32(out, w32(plan.mesh_off, "mesh offset")?);
    put32(out, w32(plan.mesh_size, "mesh size")?);
    zeros(out, MODEL_RECORD_BYTES - 280);
    Ok(())
}

/// Preset table, names and attr array of one supported model, starting at `pos`.
fn write_block(out: &mut Vec<u8>, s: &Supports, pos: u64) -> Result<(), WriteError> {
    let mut name_ptr = add(
        pos,
        mul(s.presets.len(), PRESET_ENTRY_BYTES, "preset table")?,
        "preset table",
    )?;
    for p in &s.presets {
        put64(out, p.id);
        put64(out, name_ptr);
        put64(out, p.name.len() as u64);
        name_ptr = add(name_ptr, p.name.len() as u64, "preset names")?;
    }
    for p in &s.presets {
        out.extend_from_slice(p.name.as_bytes());
    }
    for a in &s.attrs {
        put64(out, *a);
    }
    Ok(())
}

fn write_model_data(out: &mut Vec<u8>, m: &Model, plan: &ModelPlan) -> Result<(), WriteError> {
    mesh::encode_into(&m.mesh, out);
    let (Some(s), Some(sp)) = (&m.supports, &plan.sup) else {
        return Ok(());
    };
    out.extend_from_slice(&s.unreferenced_blob);
    for r in &s.records {
        mesh::encode_into(&r.mesh, out);
    }
    put32(out, w32(s.records.len() as u64, "record count")?);
    put32(out, w32(sp.records_off, "records offset")?);
    put32(out, w32(sp.blob_off, "support mesh offset")?);
    put32(out, w32(sp.blob_size, "support mesh size")?);
    zeros(out, GROUP_HEADER_BYTES);
    let mut mesh_off = add(
        sp.blob_off,
        s.unreferenced_blob.len() as u64,
        "record mesh offset",
    )?;
    for r in &s.records {
        let mesh_len = mesh::byte_len(&r.mesh, "record mesh")?;
        put32(out, RECORD_MAGIC);
        put32(out, r.record_type.0);
        for v in r.start.iter().chain(&r.end) {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&r.r1.to_le_bytes());
        out.extend_from_slice(&r.r2.to_le_bytes());
        put32(out, w32(mesh_off, "record mesh offset")?);
        put32(out, w32(mesh_len, "record mesh length")?);
        for w in r.tail {
            put32(out, w);
        }
        mesh_off = add(mesh_off, mesh_len, "record mesh offset")?;
    }
    zeros(out, TRAILER_BYTES);
    Ok(())
}

fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn zeros(out: &mut Vec<u8>, n: u64) {
    out.resize(out.len() + usize::try_from(n).unwrap_or(0), 0);
}
