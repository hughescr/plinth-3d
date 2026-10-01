//! Turn a binary STL over: rotate it 180 degrees about the X axis, exactly.
//!
//! ```text
//! cargo run -p plinth-mesh --example flip_stl_x180 -- <in.stl> <out.stl>
//! ```
//!
//! Written for the Round base fixtures (GitHub issue #1, owner spec
//! 2026-10-01): the provisioned `corpus/models/Round_{25,50,100}.stl` are drawn
//! glue face DOWN (`plinth-evidence/.../field-2026-09-28/round50.md` item 1),
//! and the owner wants them used AS DRAWN glue face UP. This tool made the
//! committed copies under `corpus/as-drawn/` ONCE; `corpus/as-drawn/README.md`
//! records the source and result hashes and the command.
//!
//! **Exact by construction.** A 180 degree turn about X maps `(x, y, z)` to
//! `(x, -y, -z)`. On the wire that is flipping the SIGN BIT of the `y` and `z`
//! f32 of each facet's normal and of its three vertices; nothing is decoded,
//! multiplied or re-rounded, so the result is bit-for-bit the rotation the
//! dense search applies when it picks the flip (its antipode convention is
//! 180 degrees about +X, `UpDirection::rotation_to_build_frame`). The header,
//! the triangle count, the triangle order, the vertex order within each
//! triangle (a rotation keeps winding) and each facet's attribute bytes are
//! copied unchanged.
//!
//! An EXAMPLE, not a test, for the reason `export_fixture` gives: it writes
//! files outside a test's own scratch.

use std::path::PathBuf;
use std::process::ExitCode;

const HEADER_BYTES: usize = 80;
const COUNT_BYTES: usize = 4;
const FACET_BYTES: usize = 50;
/// Byte offsets, within one 50-byte facet, of the f32 `y` and `z` of the
/// normal (bytes 0..12) and of the three vertices (12..24, 24..36, 36..48).
const Y_Z_OFFSETS: [usize; 8] = [4, 8, 16, 20, 28, 32, 40, 44];
/// The sign bit of a little-endian f32 is the top bit of its LAST byte.
const SIGN_BYTE: usize = 3;
const SIGN_BIT: u8 = 0x80;

fn flip(bytes: &mut [u8]) -> Result<usize, String> {
    if bytes.len() < HEADER_BYTES + COUNT_BYTES {
        return Err(format!("{} bytes is too short for a binary STL", bytes.len()));
    }
    let count = u32::from_le_bytes(
        bytes[HEADER_BYTES..HEADER_BYTES + COUNT_BYTES]
            .try_into()
            .expect("four bytes"),
    );
    let facets = usize::try_from(count).map_err(|_| "triangle count overflows".to_owned())?;
    let expected = HEADER_BYTES + COUNT_BYTES + facets * FACET_BYTES;
    if bytes.len() != expected {
        return Err(format!(
            "declares {facets} triangles ({expected} bytes) but has {} bytes (ASCII STL is not supported)",
            bytes.len()
        ));
    }
    for facet in bytes[HEADER_BYTES + COUNT_BYTES..].chunks_exact_mut(FACET_BYTES) {
        for offset in Y_Z_OFFSETS {
            facet[offset + SIGN_BYTE] ^= SIGN_BIT;
        }
    }
    Ok(facets)
}

fn main() -> ExitCode {
    let arguments: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [source, target] = arguments.as_slice() else {
        eprintln!("usage: flip_stl_x180 <in.stl> <out.stl>");
        return ExitCode::from(2);
    };
    let mut bytes = match std::fs::read(source) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("cannot read {}: {error}", source.display());
            return ExitCode::FAILURE;
        }
    };
    let facets = match flip(&mut bytes) {
        Ok(facets) => facets,
        Err(problem) => {
            eprintln!("{}: {problem}", source.display());
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = std::fs::write(target, &bytes) {
        eprintln!("cannot write {}: {error}", target.display());
        return ExitCode::FAILURE;
    }
    println!(
        "{} -> {}: {facets} facets turned 180 deg about X",
        source.display(),
        target.display()
    );
    ExitCode::SUCCESS
}
