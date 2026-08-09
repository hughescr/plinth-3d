//! Writes `docs/engineering-journal/constants.md` from the one M0 registry.
//!
//! The golden test asserts the checked-in file equals
//! `registry().render_markdown()` byte for byte, and its failure message says
//! "update the producer output, then audit the written file". This *is* that
//! producer: it never edits the Markdown, it re-emits it, so a regenerated
//! golden can only ever say what the registry says.
//!
//! `cargo run -p plinth-core --offline --example render_constants_golden`

use std::path::Path;

fn main() -> Result<(), String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("docs/engineering-journal/constants.md");
    let rendered = plinth_core::m0_constants()
        .map_err(|error| format!("M0 constants registry must construct: {error}"))?
        .registry()
        .render_markdown();
    std::fs::write(&path, &rendered)
        .map_err(|error| format!("write {}: {error}", path.display()))?;
    println!("wrote {} ({} bytes)", path.display(), rendered.len());
    Ok(())
}
