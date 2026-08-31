//! Generator for `long_ledge.chitubox`. Not built by this crate's own
//! `Cargo.toml` (there is no `[[bin]]` entry for it) -- it is a record of how
//! the fixture was produced, in the same spirit as
//! `fixtures/buried_arm/generate.rs`, `fixtures/double_load/generate.rs` and
//! `fixtures/starved_patch/generate.rs`. To regenerate: copy this file into a
//! throwaway crate with `plinth-audit` as a path dependency (see this
//! fixture's README, "Reproducing") and run it with one argument, the output
//! path.
//!
//! One model, two mesh components, **zero support records**:
//!
//! * a grounded spine block, `x in [-3, 0], y in [-1, 1], z in [0, 2]`,
//!   resting directly on the plate (`z_plate_mm = mesh.bbox.min.z -
//!   translate_z = 0 - 0 = 0`, `resolve_plate`'s `ContractRelation` branch,
//!   `src/context.rs:728`) -- no support is needed to ground it, because its
//!   own bottom layer IS the plate layer.
//! * a one-layer-thick ledge, `x in [-3, 3], y in [-1, 1], z in [1.98,
//!   2.03]` (`2.03 - 1.98 = 0.05 mm = LAYER_HEIGHT_DEFAULT_MM`, exactly one
//!   printer layer). It is sunk 0.02 mm into the top of the spine rather
//!   than sharing an exact coincident face with it -- an exact shared face
//!   welds identical `f32` vertices from both boxes onto one another
//!   (`Mesh::weld_exact`) and manufactures a spurious `FILE_MESH_NOT_CLOSED`
//!   `nonmanifold_edges` finding that has nothing to do with the planted
//!   defect (confirmed empirically: the first draft of this generator used
//!   an exact shared face at `z = 2.0` and produced exactly that). The 0.02
//!   mm sink is invisible to the single-sampling-plane raster this audit
//!   builds (`src/raster.rs`'s `layer_at_or_above`/mid-layer sampling): at
//!   layer 40's own sampling plane (`z = 2.025 mm`), only the ledge box
//!   contains that height (the spine's top is now `2.00 mm`, still below
//!   it), so the model footprint at that layer is the ledge's own, exactly
//!   as if the two boxes met at a bare shared face. Its `x in [-3, 0]` half
//!   sits directly over the spine (full footprint overlap -> attached by the
//!   per-column predicate `is_attached`, `audit-invariants.md` section 1.6 /
//!   `docs/specs/audit-span-criterion.md` section 3.1) and its `x in [0, 3]`
//!   half projects 3.00 mm past the spine's own edge with **nothing under it
//!   and no support anywhere in the file** -- the unsupported cantilever
//!   this fixture exists to plant.
//!
//! This is `docs/specs/audit-span-criterion.md` section 10's
//! `span_ledge_long` fixture, geometry-for-geometry: "flat ledge 3.00 mm
//! long, 2.00 mm wide, one layer thick, attached along one edge to a block;
//! no supports", built at this directory's assigned name (`long_ledge`) per
//! this task's own instruction rather than the spec's fixture id -- see the
//! README's "Naming" note.

use plinth_audit::synth::{box_soup, SynthFile, SynthModel};

fn main() {
    // ---- the spine (grounded directly on the plate) -----------------------
    let spine = box_soup([-3.0, -1.0, 0.0], [0.0, 1.0, 2.0]);

    // ---- the ledge: one layer thick, half over the spine, half cantilevered
    // Sunk 0.02 mm into the spine's top (see the module doc) so no vertex of
    // either box exactly coincides with a vertex of the other.
    let ledge = box_soup([-3.0, -1.0, 1.98], [3.0, 1.0, 2.03]);

    let mut mesh = spine;
    mesh.extend(ledge);

    let file = SynthFile::new().with_model(SynthModel {
        name: "long_ledge".into(),
        translate_mm: [0.0, 0.0, 0.0],
        mesh,
        records: Vec::new(),
    });

    let out = std::env::args()
        .nth(1)
        .expect("usage: gen_long_ledge <out.chitubox>");
    file.write(std::path::Path::new(&out)).expect("write");
    eprintln!("wrote {out}");
}
