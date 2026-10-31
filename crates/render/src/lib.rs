//! plinth-render: the review pictures of the written file (design
//! `step11-construction.md` section 9, pictures 1 to 12).
//!
//! It draws from the scene decoded from the written file ([`Scene`]: the model and the support
//! parts as triangles in the plate frame) and from `plinth_support::report_data::ReportData`,
//! never from the plan. A small software rasteriser (orthographic camera, z-buffer, flat
//! shading, outlines) and a PNG writer are built in, so there is no dependency.
//!
//! * [`render_pictures`] makes all twelve pictures as [`Picture`] values.
//! * [`write_all`] writes them as PNG files, each through a temporary file and a rename.

pub mod canvas;
pub mod font;
pub mod pictures;
pub mod png;
pub mod scene;

use std::path::{Path, PathBuf};

pub use canvas::{Image, Rect, View};
pub use pictures::{Picture, render_pictures};
pub use scene::{Part, Role, Scene, Tri};

/// Write each picture as `<dir>/<name>.png` (the directory is created). Returns the paths.
///
/// # Errors
/// Returns the first I/O error.
pub fn write_all(dir: &Path, pictures: &[Picture]) -> std::io::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dir)?;
    let mut out = Vec::with_capacity(pictures.len());
    for p in pictures {
        let path = dir.join(format!("{}.png", p.name));
        let tmp = dir.join(format!(".{}.png.tmp", p.name));
        std::fs::write(&tmp, p.image.to_png())?;
        std::fs::rename(&tmp, &path)?;
        out.push(path);
    }
    Ok(out)
}
