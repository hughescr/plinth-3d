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
//! committed copies under `corpus/as-drawn/` once, on 2026-10-01. Since
//! 2026-10-02 the whole folder is made, and checked, from its recipe
//! `corpus/as-drawn/sources.tsv` by the `as_drawn_corpus` example; this one
//! turns a single file over for anything else.
//!
//! **Exact by construction.** The turn is `plinth_mesh::turn_binary_stl_x180`:
//! `(x, y, z) -> (x, -y, -z)` as a sign-bit flip of the `y` and `z` f32 of each
//! facet's normal and vertices, with nothing decoded or re-rounded. That is bit
//! for bit the rotation the dense search applies when it picks the flip (its
//! antipode convention is 180 degrees about +X,
//! `UpDirection::rotation_to_build_frame`).
//!
//! An EXAMPLE, not a test, for the reason `export_fixture` gives: it writes
//! files outside a test's own scratch.

use std::path::PathBuf;
use std::process::ExitCode;

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
    let facets = match plinth_mesh::turn_binary_stl_x180(source, &mut bytes) {
        Ok(facets) => facets,
        Err(problem) => {
            eprintln!("{problem}");
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
