use std::fs;
use std::path::{Path, PathBuf};

use geom_kernel::vec3::{cross, dot, normalize, sub};
use thiserror::Error;

use crate::{Mesh, MeshError, WeldPolicy};

const HEADER_BYTES: usize = 80;
const COUNT_BYTES: usize = 4;
const FACET_BYTES: usize = 50;

/// Semantic and I/O failures for binary STL operations.
#[derive(Debug, Error)]
pub enum MeshIoError {
    #[error("could not read STL {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not write STL {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path} is too short to be a binary STL ({actual} bytes)")]
    TooShort { path: PathBuf, actual: usize },
    #[error("{path} looks like ASCII STL; only binary STL is supported")]
    AsciiStlUnsupported { path: PathBuf },
    #[error("{path} declares {triangles} triangles, which overflows the binary STL size")]
    CountOverflow { path: PathBuf, triangles: u32 },
    #[error(
        "{path} declares {triangles} triangles ({expected} bytes) but contains {actual} bytes"
    )]
    LengthMismatch {
        path: PathBuf,
        triangles: u32,
        expected: usize,
        actual: usize,
    },
    #[error("{path} declares zero triangles")]
    ZeroTriangles { path: PathBuf },
    #[error("{path} facet {facet} {field} coordinate {coordinate} is not finite")]
    NonFinite {
        path: PathBuf,
        facet: usize,
        field: &'static str,
        coordinate: usize,
    },
    #[error("STL IO cannot apply requested welding policy {policy:?}; M2 currently supports only WeldPolicy::None")]
    UnsupportedWeldPolicy { policy: WeldPolicy },
    #[error("STL output {path} has {triangles} triangles, more than u32 permits")]
    TooManyTriangles { path: PathBuf, triangles: usize },
    #[error("STL output {path} triangle {triangle} uses f64 coordinate {coordinate} that cannot be represented exactly as f32")]
    LossyF32Encoding {
        path: PathBuf,
        triangle: usize,
        coordinate: usize,
    },
    #[error(transparent)]
    Mesh(#[from] MeshError),
}

/// Load one binary STL with a caller-selected, recorded welding policy.
pub fn load_binary_stl(
    path: impl AsRef<Path>,
    weld_policy: WeldPolicy,
) -> Result<Mesh, MeshIoError> {
    require_unwelded(weld_policy)?;
    let path = path.as_ref();
    let bytes = fs::read(path).map_err(|source| MeshIoError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    decode_binary_stl(path, &bytes, weld_policy)
}

/// Load a batch of binary STLs in input order.
pub fn load_binary_stls(
    paths: &[PathBuf],
    weld_policy: WeldPolicy,
) -> Result<Vec<Mesh>, MeshIoError> {
    require_unwelded(weld_policy)?;
    paths
        .iter()
        .map(|path| load_binary_stl(path, weld_policy))
        .collect()
}

/// Write one binary STL.  f64 values must round-trip exactly through STL's f32 wire value.
pub fn write_binary_stl(path: impl AsRef<Path>, mesh: &Mesh) -> Result<(), MeshIoError> {
    let path = path.as_ref();
    mesh.validate()?;
    let bytes = encode_binary_stl(path, mesh)?;
    fs::write(path, bytes).map_err(|source| MeshIoError::Write {
        path: path.to_path_buf(),
        source,
    })
}

/// Write a gathered batch of `(path, mesh)` pairs in caller order.
pub fn write_binary_stls(entries: &[(PathBuf, Mesh)]) -> Result<(), MeshIoError> {
    for (path, mesh) in entries {
        write_binary_stl(path, mesh)?;
    }
    Ok(())
}

fn decode_binary_stl(
    path: &Path,
    bytes: &[u8],
    weld_policy: WeldPolicy,
) -> Result<Mesh, MeshIoError> {
    require_unwelded(weld_policy)?;
    if starts_with_solid(bytes) && !looks_like_binary_stl(bytes) {
        return Err(MeshIoError::AsciiStlUnsupported {
            path: path.to_path_buf(),
        });
    }
    if bytes.len() < HEADER_BYTES + COUNT_BYTES {
        return Err(MeshIoError::TooShort {
            path: path.to_path_buf(),
            actual: bytes.len(),
        });
    }
    let triangles = u32::from_le_bytes(
        bytes[HEADER_BYTES..HEADER_BYTES + COUNT_BYTES]
            .try_into()
            .expect("checked width"),
    );
    let expected = expected_len(path, triangles)?;
    if bytes.len() != expected {
        return Err(MeshIoError::LengthMismatch {
            path: path.to_path_buf(),
            triangles,
            expected,
            actual: bytes.len(),
        });
    }
    if triangles == 0 {
        return Err(MeshIoError::ZeroTriangles {
            path: path.to_path_buf(),
        });
    }
    let triangle_count = usize::try_from(triangles).expect("u32 fits usize on supported targets");
    let vertex_count = triangle_count
        .checked_mul(3)
        .ok_or_else(|| MeshIoError::CountOverflow {
            path: path.to_path_buf(),
            triangles,
        })?;
    let mut verts = Vec::with_capacity(vertex_count);
    let mut tris = Vec::with_capacity(triangle_count);
    for facet in 0..triangle_count {
        let offset = HEADER_BYTES + COUNT_BYTES + facet * FACET_BYTES;
        for coordinate in 0..3 {
            let value = f32_at(bytes, offset + coordinate * 4);
            require_finite(path, facet, "normal", coordinate, value)?;
        }
        let mut corners = [[0.0; 3]; 3];
        for (corner, values) in corners.iter_mut().enumerate() {
            for (axis, value) in values.iter_mut().enumerate() {
                let decoded = f32_at(bytes, offset + 12 + (corner * 3 + axis) * 4);
                require_finite(path, facet, "vertex", corner * 3 + axis, decoded)?;
                *value = f64::from(decoded);
            }
        }
        let base = u32::try_from(verts.len()).map_err(|_| MeshIoError::CountOverflow {
            path: path.to_path_buf(),
            triangles,
        })?;
        let one = base
            .checked_add(1)
            .ok_or_else(|| MeshIoError::CountOverflow {
                path: path.to_path_buf(),
                triangles,
            })?;
        let two = base
            .checked_add(2)
            .ok_or_else(|| MeshIoError::CountOverflow {
                path: path.to_path_buf(),
                triangles,
            })?;
        verts.extend(corners);
        tris.push([base, one, two]);
    }
    Mesh::new_unwelded(verts, tris).map_err(Into::into)
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "deliberate narrowing to the binary32 the printer file format stores. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn encode_binary_stl(path: &Path, mesh: &Mesh) -> Result<Vec<u8>, MeshIoError> {
    let triangle_count = mesh.tris.len();
    let count = u32::try_from(triangle_count).map_err(|_| MeshIoError::TooManyTriangles {
        path: path.to_path_buf(),
        triangles: triangle_count,
    })?;
    let len = expected_len(path, count)?;
    let mut bytes = vec![0_u8; len];
    bytes[..25].copy_from_slice(b"plinth-mesh binary STL v1");
    bytes[HEADER_BYTES..HEADER_BYTES + COUNT_BYTES].copy_from_slice(&count.to_le_bytes());
    for (triangle, indices) in mesh.tris.iter().enumerate() {
        let offset = HEADER_BYTES + COUNT_BYTES + triangle * FACET_BYTES;
        let normal = unit_normal(mesh, triangle, *indices)?;
        for (coordinate, value) in normal.iter().enumerate() {
            put_f32(&mut bytes, offset + coordinate * 4, *value as f32);
        }
        for (corner, index) in indices.iter().enumerate() {
            let vertex = mesh.verts[*index as usize];
            for (axis, value) in vertex.iter().enumerate() {
                let encoded = *value as f32;
                if !encoded.is_finite() || f64::from(encoded).to_bits() != value.to_bits() {
                    return Err(MeshIoError::LossyF32Encoding {
                        path: path.to_path_buf(),
                        triangle,
                        coordinate: corner * 3 + axis,
                    });
                }
                put_f32(&mut bytes, offset + 12 + (corner * 3 + axis) * 4, encoded);
            }
        }
        // Attribute count remains intentionally zero.  It is outside array identity.
    }
    Ok(bytes)
}

fn require_unwelded(policy: WeldPolicy) -> Result<(), MeshIoError> {
    if policy == WeldPolicy::None {
        Ok(())
    } else {
        Err(MeshIoError::UnsupportedWeldPolicy { policy })
    }
}

fn expected_len(path: &Path, triangles: u32) -> Result<usize, MeshIoError> {
    let body = usize::try_from(triangles)
        .ok()
        .and_then(|count| count.checked_mul(FACET_BYTES));
    (HEADER_BYTES + COUNT_BYTES)
        .checked_add(body.ok_or_else(|| MeshIoError::CountOverflow {
            path: path.to_path_buf(),
            triangles,
        })?)
        .ok_or_else(|| MeshIoError::CountOverflow {
            path: path.to_path_buf(),
            triangles,
        })
}

fn starts_with_solid(bytes: &[u8]) -> bool {
    bytes
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"solid"))
}

fn looks_like_binary_stl(bytes: &[u8]) -> bool {
    if bytes.len() < HEADER_BYTES + COUNT_BYTES {
        return false;
    }
    let triangles = u32::from_le_bytes(
        bytes[HEADER_BYTES..HEADER_BYTES + COUNT_BYTES]
            .try_into()
            .expect("checked width"),
    );
    expected_len(Path::new("binary STL"), triangles).is_ok_and(|expected| expected == bytes.len())
}

fn f32_at(bytes: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("validated STL length"),
    )
}

fn put_f32(bytes: &mut [u8], offset: usize, value: f32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn require_finite(
    path: &Path,
    facet: usize,
    field: &'static str,
    coordinate: usize,
    value: f32,
) -> Result<(), MeshIoError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(MeshIoError::NonFinite {
            path: path.to_path_buf(),
            facet,
            field,
            coordinate,
        })
    }
}

fn unit_normal(mesh: &Mesh, triangle: usize, indices: [u32; 3]) -> Result<[f64; 3], MeshIoError> {
    let a = mesh.verts[indices[0] as usize];
    let b = mesh.verts[indices[1] as usize];
    let c = mesh.verts[indices[2] as usize];
    // Canonical face normal: `geom_kernel::vec3` cross + `normalize`
    // (docs/specs/geometry-canon.md §1 OP-4 and OP-6, triage rule (d)).
    // `normalize` refuses on exactly this site's two former refusals — a
    // non-finite squared norm and a zero-length cross product — so the two
    // typed errors are separated by re-testing the squared norm, which keeps
    // `NormalArithmeticOverflow` and `DegenerateTriangle` distinguishable.
    //
    // The SPELLING moves bits: the kernel multiplies by one reciprocal where
    // this function divided three times (kernel `normalize` doc, "Spelling,
    // and the ULP it costs"). Measured over the eight-model corpus, 27.26 % of
    // f64 normal components move, all by exactly 1 ULP (<= 1.110e-16) — and
    // **0 of 1,303,152 differ after the f32 cast this function's caller
    // applies**, so no emitted STL byte moves.
    let normal = cross(sub(b, a), sub(c, a));
    if !dot(normal, normal).is_finite() {
        return Err(MeshError::NormalArithmeticOverflow { triangle }.into());
    }
    // With the squared norm known finite, `normalize` is `None` on exactly the
    // remaining former refusal: a zero-length cross product.
    normalize(normal).ok_or_else(|| MeshError::DegenerateTriangle { triangle }.into())
}
