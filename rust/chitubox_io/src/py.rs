//! PyO3 module `chitubox_io._native` — the Python boundary onto this crate.
//!
//! DOCTRINE (why this shape, not another)
//!
//! * **Empty-template only.** There is no `template` / `from_bytes` parameter
//!   anywhere on this surface. Every project is built with
//!   [`Project::new_empty`], then the model and its supports are written
//!   explicitly. The "correct supports stapled onto the WRONG MODEL" failure
//!   mode of the clone-and-patch Python writer is removed *structurally* —
//!   there is no code path here that can carry a foreign mesh into an output.
//!
//! * **Batch-first, columnar.** One call produces one file. Support records
//!   cross the boundary as parallel numpy arrays plus ONE concatenated
//!   `mesh_flat` buffer with a per-record float count — never as a list of a
//!   thousand Python objects. The treant-scale payload is ~16 MB of triangle
//!   soup; a per-record crossing there would be a defect, not a style choice.
//!
//! * **Copy under the GIL, compute without it.** Every numpy buffer is copied
//!   into Rust-owned storage while the GIL is held; the container build,
//!   verify, and save then run inside `Python::allow_threads`. No numpy view is
//!   ever read outside the GIL.
//!
//! * **Typed errors.** Every `ChituboxError` becomes a
//!   `chitubox_io.ChituboxError` whose `str()` is the `thiserror` message and
//!   whose `.kind` is the variant name (`"NonFiniteValue"`, `"VerifyFailed"`,
//!   …). Callers branch on `.kind`, never on message text.

use std::path::PathBuf;

use numpy::{PyReadonlyArray1, PyReadonlyArray2, PyReadonlyArray3, PyUntypedArrayMethods};
use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::error::ChituboxError;
use crate::project::Project;
use crate::types::{
    ModelGeometry, ModelSource, Placement, SegmentType, SupportBatch, SupportMode, SupportRecord,
    TriangleSoup,
};
use crate::CHITUBOX_API_VERSION;

create_exception!(_native, ChituboxErrorPy, PyException);

/// Stable discriminant string for a crate error. Exhaustive on purpose: a new
/// variant must be given a name here rather than silently becoming "Unknown".
fn kind_of(e: &ChituboxError) -> &'static str {
    match e {
        ChituboxError::Io(_) => "Io",
        ChituboxError::NotChitubox { .. } => "NotChitubox",
        ChituboxError::Truncated { .. } => "Truncated",
        ChituboxError::HeaderInvariant { .. } => "HeaderInvariant",
        ChituboxError::ModelIndexOutOfRange { .. } => "ModelIndexOutOfRange",
        ChituboxError::BadTriangleBuffer { .. } => "BadTriangleBuffer",
        ChituboxError::NonFiniteValue { .. } => "NonFiniteValue",
        ChituboxError::StlParse { .. } => "StlParse",
        ChituboxError::AsciiStlUnsupported => "AsciiStlUnsupported",
        ChituboxError::SupportSectionMalformed { .. } => "SupportSectionMalformed",
        ChituboxError::SupportPointerInvalid { .. } => "SupportPointerInvalid",
        ChituboxError::NameInvalid { .. } => "NameInvalid",
        ChituboxError::DuplicateBatchIndex { .. } => "DuplicateBatchIndex",
        ChituboxError::ArithmeticOverflow { .. } => "ArithmeticOverflow",
        ChituboxError::FileTooLarge { .. } => "FileTooLarge",
        ChituboxError::OpaqueLengthMismatch { .. } => "OpaqueLengthMismatch",
        ChituboxError::VerifyFailed { .. } => "VerifyFailed",
        ChituboxError::FormatDrift { .. } => "FormatDrift",
        ChituboxError::UnknownOpaqueRegion(_) => "UnknownOpaqueRegion",
    }
}

/// `ChituboxError` -> a Python exception carrying `.kind`.
fn to_pyerr(e: ChituboxError) -> PyErr {
    let kind = kind_of(&e);
    let err = ChituboxErrorPy::new_err(e.to_string());
    Python::with_gil(|py| {
        // Best-effort: the exception instance always has a __dict__, so this
        // cannot realistically fail; if it ever did, the message still carries
        // the full diagnosis and only `.kind` would be missing.
        let _ = err.value(py).setattr("kind", kind);
    });
    err
}

/// Shorthand for an input-shape complaint (a caller bug, not a format error).
fn bad_input(msg: impl Into<String>) -> PyErr {
    PyValueError::new_err(msg.into())
}

// ---------------------------------------------------------------------------
// numpy intake helpers. Each copies into Rust-owned storage under the GIL.
// ---------------------------------------------------------------------------

fn rows3(a: &PyReadonlyArray2<f32>, field: &str, n: usize) -> PyResult<Vec<[f32; 3]>> {
    let s = a.shape();
    if s[0] != n || s[1] != 3 {
        return Err(bad_input(format!(
            "supports['{field}'] must have shape ({n},3), got {s:?}"
        )));
    }
    let flat = a
        .as_slice()
        .map_err(|_| bad_input(format!("supports['{field}'] must be C-contiguous")))?;
    Ok(flat.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect())
}

fn rows2(a: &PyReadonlyArray2<f32>, field: &str, n: usize) -> PyResult<Vec<[f32; 2]>> {
    let s = a.shape();
    if s[0] != n || s[1] != 2 {
        return Err(bad_input(format!(
            "supports['{field}'] must have shape ({n},2), got {s:?}"
        )));
    }
    let flat = a
        .as_slice()
        .map_err(|_| bad_input(format!("supports['{field}'] must be C-contiguous")))?;
    Ok(flat.chunks_exact(2).map(|c| [c[0], c[1]]).collect())
}

fn rows24(a: &PyReadonlyArray2<u8>, field: &str, n: usize) -> PyResult<Vec<[u8; 24]>> {
    let s = a.shape();
    if s[0] != n || s[1] != 24 {
        return Err(bad_input(format!(
            "supports['{field}'] must have shape ({n},24), got {s:?}"
        )));
    }
    let flat = a
        .as_slice()
        .map_err(|_| bad_input(format!("supports['{field}'] must be C-contiguous")))?;
    Ok(flat
        .chunks_exact(24)
        .map(|c| {
            let mut out = [0u8; 24];
            out.copy_from_slice(c);
            out
        })
        .collect())
}

fn vec1<'py, T>(a: &'py PyReadonlyArray1<T>, field: &str) -> PyResult<&'py [T]>
where
    T: numpy::Element,
{
    a.as_slice()
        .map_err(|_| bad_input(format!("supports['{field}'] must be C-contiguous")))
}

/// Flatten an (n_tri,3,3) float32 array into the crate's 9-f32/triangle stream.
fn triangles_flat(a: &PyReadonlyArray3<f32>, field: &str) -> PyResult<Vec<f32>> {
    let s = a.shape();
    if s[1] != 3 || s[2] != 3 {
        return Err(bad_input(format!(
            "'{field}' must have shape (n_tri,3,3), got {s:?}"
        )));
    }
    let flat = a
        .as_slice()
        .map_err(|_| bad_input(format!("'{field}' must be C-contiguous")))?;
    Ok(flat.to_vec())
}

// ---------------------------------------------------------------------------
// model-spec intake
// ---------------------------------------------------------------------------

/// One fully-decoded model: geometry + placement + its (optional) support
/// records, all Rust-owned so the write can run with the GIL released.
struct DecodedModel {
    source: ModelSource,
    records: Vec<SupportRecord>,
}

fn decode_supports(d: &Bound<'_, PyDict>) -> PyResult<Vec<SupportRecord>> {
    let get = |k: &str| -> PyResult<Bound<'_, PyAny>> {
        d.get_item(k)?
            .ok_or_else(|| bad_input(format!("supports dict is missing '{k}'")))
    };

    let seg_any = get("seg_type")?;
    let seg_arr: PyReadonlyArray1<u32> = seg_any.extract().map_err(|_| {
        bad_input("supports['seg_type'] must be a 1-D uint32 array")
    })?;
    let seg = vec1(&seg_arr, "seg_type")?;
    let n = seg.len();

    let start_any = get("start")?;
    let start_arr: PyReadonlyArray2<f32> = start_any
        .extract()
        .map_err(|_| bad_input("supports['start'] must be a float32 (n,3) array"))?;
    let start = rows3(&start_arr, "start", n)?;

    let end_any = get("end")?;
    let end_arr: PyReadonlyArray2<f32> = end_any
        .extract()
        .map_err(|_| bad_input("supports['end'] must be a float32 (n,3) array"))?;
    let end = rows3(&end_arr, "end", n)?;

    let radii_any = get("radii")?;
    let radii_arr: PyReadonlyArray2<f32> = radii_any
        .extract()
        .map_err(|_| bad_input("supports['radii'] must be a float32 (n,2) array"))?;
    let radii = rows2(&radii_arr, "radii", n)?;

    let trailing_any = get("trailing")?;
    let trailing_arr: PyReadonlyArray2<u8> = trailing_any
        .extract()
        .map_err(|_| bad_input("supports['trailing'] must be a uint8 (n,24) array"))?;
    let trailing = rows24(&trailing_arr, "trailing", n)?;

    let mesh_len_any = get("mesh_len")?;
    let mesh_len_arr: PyReadonlyArray1<u32> = mesh_len_any
        .extract()
        .map_err(|_| bad_input("supports['mesh_len'] must be a 1-D uint32 array"))?;
    let mesh_len = vec1(&mesh_len_arr, "mesh_len")?;
    if mesh_len.len() != n {
        return Err(bad_input(format!(
            "supports['mesh_len'] has {} entries, expected {n}",
            mesh_len.len()
        )));
    }

    let mesh_any = get("mesh_flat")?;
    let mesh_arr: PyReadonlyArray1<f32> = mesh_any
        .extract()
        .map_err(|_| bad_input("supports['mesh_flat'] must be a 1-D float32 array"))?;
    let mesh = vec1(&mesh_arr, "mesh_flat")?;

    // ONE pass, prefix-summing mesh_len into slices of the single concatenated
    // buffer. The total is checked exactly: a short or long mesh_flat is a
    // caller bug that would otherwise silently truncate or duplicate geometry.
    let mut records = Vec::with_capacity(n);
    let mut off: usize = 0;
    for i in 0..n {
        let len = mesh_len[i] as usize;
        let end_off = off.checked_add(len).ok_or_else(|| {
            bad_input("supports['mesh_len'] prefix sum overflows usize")
        })?;
        if end_off > mesh.len() {
            return Err(bad_input(format!(
                "supports['mesh_flat'] is too short: record {i} needs floats \
                 {off}..{end_off} but the buffer holds {}",
                mesh.len()
            )));
        }
        records.push(SupportRecord {
            segment_type: SegmentType(seg[i]),
            start: start[i],
            end: end[i],
            r_start: radii[i][0],
            r_end: radii[i][1],
            mesh: TriangleSoup::new(mesh[off..end_off].to_vec()),
            trailing: trailing[i],
        });
        off = end_off;
    }
    if off != mesh.len() {
        return Err(bad_input(format!(
            "supports['mesh_flat'] has {} floats but mesh_len accounts for {off}",
            mesh.len()
        )));
    }
    Ok(records)
}

fn decode_model(item: &Bound<'_, PyAny>) -> PyResult<DecodedModel> {
    let d: &Bound<'_, PyDict> = item
        .downcast()
        .map_err(|_| bad_input("each entry of `models` must be a dict"))?;

    let name: String = d
        .get_item("name")?
        .ok_or_else(|| bad_input("model dict is missing 'name'"))?
        .extract()?;

    let stl = d.get_item("stl_path")?;
    let tris = d.get_item("triangles")?;
    let geometry = match (stl.as_ref(), tris.as_ref()) {
        (Some(p), None) => {
            let s: String = p.extract().map_err(|_| bad_input("'stl_path' must be a str"))?;
            ModelGeometry::StlPath(PathBuf::from(s))
        }
        (None, Some(t)) => {
            let arr: PyReadonlyArray3<f32> = t
                .extract()
                .map_err(|_| bad_input("'triangles' must be a float32 (n_tri,3,3) array"))?;
            ModelGeometry::Triangles(TriangleSoup::new(triangles_flat(&arr, "triangles")?))
        }
        _ => {
            return Err(bad_input(
                "model dict needs exactly one of 'stl_path' or 'triangles'",
            ))
        }
    };

    let placement: (f32, f32, f32) = d
        .get_item("placement")?
        .ok_or_else(|| bad_input("model dict is missing 'placement' (x, y, z_lift)"))?
        .extract()
        .map_err(|_| bad_input("'placement' must be a 3-tuple of floats (x, y, z_lift)"))?;

    let records = match d.get_item("supports")? {
        None => Vec::new(),
        Some(v) if v.is_none() => Vec::new(),
        Some(v) => {
            let sd: &Bound<'_, PyDict> = v
                .downcast()
                .map_err(|_| bad_input("'supports' must be a dict of parallel arrays"))?;
            decode_supports(sd)?
        }
    };

    Ok(DecodedModel {
        source: ModelSource {
            name,
            geometry,
            placement: Placement {
                x: placement.0,
                y: placement.1,
                z_lift: placement.2,
            },
        },
        records,
    })
}

// ---------------------------------------------------------------------------
// public functions
// ---------------------------------------------------------------------------

/// What a completed write measured about itself.
struct WriteOutcome {
    bytes_written: u64,
    model_count: usize,
    support_counts: Vec<usize>,
    reparse_model_count: usize,
    reparse_triangle_counts: Vec<usize>,
    reparse_support_counts: Vec<usize>,
}

/// Build a project from EMPTY, add every model, attach every support batch,
/// save, then re-open the saved file and measure what came back.
fn build_and_save(out_path: &str, decoded: Vec<DecodedModel>) -> Result<WriteOutcome, ChituboxError> {
    let mut proj = Project::new_empty();
    let sources: Vec<ModelSource> = decoded.iter().map(|m| m.source.clone()).collect();
    let indices = proj.add_models(&sources)?;

    // One batch per model that actually has records; `lift_mm: None` because
    // the z lift already travelled in `placement.z_lift` — one source of truth,
    // so the two can never disagree.
    let batches: Vec<SupportBatch> = decoded
        .into_iter()
        .zip(indices.iter())
        .filter(|(m, _)| !m.records.is_empty())
        .map(|(m, &idx)| SupportBatch {
            model_index: idx,
            records: m.records,
            mode: SupportMode::Replace,
            lift_mm: None,
        })
        .collect();
    if !batches.is_empty() {
        proj.attach_supports(&batches)?;
    }

    let path = std::path::Path::new(out_path);
    proj.save(path)?;
    let bytes_written = std::fs::metadata(path)?.len();

    // Independent read-back. `save` already structurally verifies the bytes it
    // produced; this additionally proves the bytes that reached the filesystem
    // parse, and reports what they contain rather than asserting it.
    let re = Project::open(path)?;
    let re_models = re.models();
    Ok(WriteOutcome {
        bytes_written,
        model_count: proj.model_count(),
        support_counts: proj.support_counts(),
        reparse_model_count: re.model_count(),
        reparse_triangle_counts: re_models.iter().map(|m| m.triangle_count).collect(),
        reparse_support_counts: re.support_counts(),
    })
}

/// Write ONE `.chitubox` from an empty template.
///
/// `models` is a list of dicts; see the module docstring of
/// `support_placement.emit` for the canonical producer.
#[pyfunction]
fn write_project(py: Python<'_>, out_path: &str, models: &Bound<'_, PyList>) -> PyResult<PyObject> {
    if models.is_empty() {
        return Err(bad_input("`models` must contain at least one model"));
    }
    // -- copy every buffer into Rust-owned storage while the GIL is held --
    let mut decoded = Vec::with_capacity(models.len());
    for item in models.iter() {
        decoded.push(decode_model(&item)?);
    }

    // -- container build + verify + save, GIL released --
    let owned_path = out_path.to_string();
    let outcome = py
        .allow_threads(move || build_and_save(&owned_path, decoded))
        .map_err(to_pyerr)?;

    let reparse_ok = outcome.reparse_model_count == outcome.model_count
        && outcome.reparse_support_counts == outcome.support_counts;

    let d = PyDict::new(py);
    d.set_item("path", out_path)?;
    d.set_item("bytes_written", outcome.bytes_written)?;
    d.set_item("model_count", outcome.model_count)?;
    d.set_item("support_counts", outcome.support_counts)?;
    d.set_item("triangle_counts", outcome.reparse_triangle_counts)?;
    // Measured, not asserted: the caller decides what a False means. See
    // `support_placement.emit.emit_project`, which refuses to return a report
    // for a file whose read-back disagreed with what it wrote.
    d.set_item("reparse_ok", reparse_ok)?;
    Ok(d.into())
}

/// Read a `.chitubox` back as plain Python data — the test oracle. Returns
/// structure and per-record geometry, never mesh blobs (a treant-scale file
/// would be ~16 MB of triangles that no assertion needs).
#[pyfunction]
fn read_project(py: Python<'_>, path: &str) -> PyResult<PyObject> {
    let owned = path.to_string();
    let (models, supports) = py
        .allow_threads(move || -> Result<_, ChituboxError> {
            let proj = Project::open(std::path::Path::new(&owned))?;
            let models = proj.models();
            let mut supports = Vec::with_capacity(models.len());
            for m in &models {
                supports.push(proj.supports(m.index)?);
            }
            Ok((models, supports))
        })
        .map_err(to_pyerr)?;

    let out_models = PyList::empty(py);
    for (m, recs) in models.iter().zip(supports.iter()) {
        let md = PyDict::new(py);
        md.set_item("index", m.index)?;
        md.set_item("name", &m.name)?;
        md.set_item(
            "placement",
            (m.placement.x, m.placement.y, m.placement.z_lift),
        )?;
        md.set_item("triangle_count", m.triangle_count)?;
        md.set_item("has_supports", m.has_supports)?;
        md.set_item("support_record_count", m.support_record_count)?;
        md.set_item("bbox_min", m.local_bbox_min.to_vec())?;
        md.set_item("bbox_max", m.local_bbox_max.to_vec())?;
        let rl = PyList::empty(py);
        for r in recs {
            let rd = PyDict::new(py);
            rd.set_item("seg_type", r.segment_type.0)?;
            rd.set_item("start", r.start.to_vec())?;
            rd.set_item("end", r.end.to_vec())?;
            rd.set_item("radii", (r.r_start, r.r_end))?;
            rd.set_item("triangle_count", r.mesh.triangle_count())?;
            rl.append(rd)?;
        }
        md.set_item("supports", rl)?;
        out_models.append(md)?;
    }

    let d = PyDict::new(py);
    d.set_item("path", path)?;
    d.set_item("model_count", models.len())?;
    d.set_item("models", out_models)?;
    Ok(d.into())
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("API_VERSION", CHITUBOX_API_VERSION)?;
    m.add("SOURCE_DIGEST", env!("CHITUBOX_IO_SOURCE_DIGEST"))?;
    m.add("ChituboxError", m.py().get_type::<ChituboxErrorPy>())?;
    m.add_function(wrap_pyfunction!(write_project, m)?)?;
    m.add_function(wrap_pyfunction!(read_project, m)?)?;
    Ok(())
}
