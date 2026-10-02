//! **The committed "use as drawn" fixtures, `corpus/as-drawn/`** (owner,
//! 2026-10-02: the test set uses his orientation choices, marked in the
//! repository because git keeps no Finder tag).
//!
//! Two properties, each on the files exactly as committed:
//!
//! 1. **Reproducible, never hand-edited.** Every `<stem>.stl` in the folder
//!    has one row in `corpus/as-drawn/sources.tsv`, its recorded sha256, and
//!    an empty `<stem>.asdrawn` marker that the PRODUCTION probe
//!    (`plinth_orient::as_drawn::probe_source`, the call `orient_model` makes)
//!    reads as the mark. Regenerated from its provisioned source by the
//!    library turn (`plinth_mesh::turn_binary_stl_x180`, the one the
//!    `as_drawn_corpus` example writes with), it is byte for byte the committed
//!    file.
//! 2. **The pose the owner asked for.** Used as drawn the way production uses
//!    a marked model (load, repair pre-pass, registered decimation,
//!    `orient_as_drawn`), the overhang table stands on its LEGS and the raking
//!    blade's blade points UP. The provisioned raking blade is checked to be
//!    drawn blade DOWN, which is why its committed copy is turned over. The
//!    Round discs' glue-face-up pose is graded in `plinth-sweep`'s
//!    `round_bases_ratchet`.
//!
//! The provisioned sources are gitignored `corpus/models/*.stl`; like the other
//! corpus tests, these FAIL (saying so) on a checkout that has not provisioned
//! them (`docs/development.md`, "Corpus provisioning").

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use plinth_core::m0_constants;
use plinth_decimate::{BudgetConfigError, DecimationBudget, ProbeConfig};
use plinth_mesh::Mesh;
use plinth_orient::as_drawn::{marker_path, probe_source, AsDrawnReason};
use plinth_orient::{orient_as_drawn, BaselineSelection};
use sha2::{Digest, Sha256};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn as_drawn_dir() -> PathBuf {
    repo_root().join("corpus/as-drawn")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Turn {
    X180,
    None,
}

#[derive(Debug)]
struct Row {
    stem: String,
    source: PathBuf,
    turn: Turn,
    source_sha256: String,
    result_sha256: String,
}

/// `sources.tsv`, read as the recipe it is: `#` lines are comments, every
/// other line is exactly five tab-separated fields.
fn rows() -> Vec<Row> {
    let path = as_drawn_dir().join("sources.tsv");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            let [stem, source, turn, source_sha256, result_sha256] = fields.as_slice() else {
                panic!("{}: not five tab-separated fields: {line:?}", path.display());
            };
            Row {
                stem: (*stem).to_owned(),
                source: repo_root().join(source),
                turn: match *turn {
                    "x180" => Turn::X180,
                    "none" => Turn::None,
                    other => panic!("{}: unknown turn {other:?}", path.display()),
                },
                source_sha256: (*source_sha256).to_owned(),
                result_sha256: (*result_sha256).to_owned(),
            }
        })
        .collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn committed(stem: &str) -> PathBuf {
    as_drawn_dir().join(format!("{stem}.stl"))
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|error| {
        panic!(
            "cannot read {}: {error} (a corpus/models source must be provisioned: \
             docs/development.md, \"Corpus provisioning\")",
            path.display()
        )
    })
}

#[test]
fn every_committed_as_drawn_file_is_its_recorded_turn_of_its_source_and_is_marked() {
    let rows = rows();
    let listed: BTreeSet<String> = rows.iter().map(|row| row.stem.clone()).collect();
    assert_eq!(listed.len(), rows.len(), "a stem is listed twice in sources.tsv");
    let on_disk: BTreeSet<String> = std::fs::read_dir(as_drawn_dir())
        .expect("corpus/as-drawn")
        .filter_map(|entry| {
            let path = entry.expect("dir entry").path();
            (path.extension().and_then(|ext| ext.to_str()) == Some("stl"))
                .then(|| path.file_stem().expect("stem").to_string_lossy().into_owned())
        })
        .collect();
    assert_eq!(
        on_disk, listed,
        "every STL in corpus/as-drawn must have exactly one sources.tsv row, and every row its STL"
    );

    let (mut marked, mut turned, mut copied) = (0_usize, 0_usize, 0_usize);
    for row in &rows {
        let path = committed(&row.stem);
        let bytes = read(&path);
        assert_eq!(
            sha256_hex(&bytes),
            row.result_sha256,
            "{}: the committed file is not the one sources.tsv records",
            row.stem
        );

        let marker = marker_path(&path);
        let marker_bytes = read(&marker);
        assert!(marker_bytes.is_empty(), "{}: the marker must be empty", row.stem);
        let probe = probe_source(&path);
        assert!(probe.warnings.is_empty(), "{}: {:?}", row.stem, probe.warnings);
        match probe.reason {
            Some(AsDrawnReason::MarkerFile { marker: found }) => {
                assert_eq!(found, marker, "{}: the probe read another marker", row.stem);
                marked += 1;
            }
            other => panic!(
                "{}: the production probe must mark it by its <stem>.asdrawn marker; it read \
                 {other:?}",
                row.stem
            ),
        }

        let source = read(&row.source);
        assert_eq!(
            sha256_hex(&source),
            row.source_sha256,
            "{}: the provisioned source {} is not the one sources.tsv records",
            row.stem,
            row.source.display()
        );
        let mut regenerated = source.clone();
        match row.turn {
            Turn::X180 => {
                let facets = plinth_mesh::turn_binary_stl_x180(&row.source, &mut regenerated)
                    .unwrap_or_else(|error| panic!("{}: {error}", row.stem));
                let differing = source
                    .iter()
                    .zip(&regenerated)
                    .filter(|(a, b)| a != b)
                    .count();
                assert_eq!(
                    differing,
                    8 * facets,
                    "{}: a turn about X changes exactly the sign byte of y and z in each facet's \
                     normal and three vertices",
                    row.stem
                );
                turned += 1;
            }
            Turn::None => copied += 1,
        }
        assert!(
            regenerated == bytes,
            "{}: regenerated from {} by turn {:?}, the file is not byte for byte the committed one",
            row.stem,
            row.source.display(),
            row.turn
        );
    }
    println!(
        "as-drawn corpus: {} rows; {marked} marked by the production probe; {turned} regenerated \
         by turn_binary_stl_x180, {copied} byte copies; all byte-identical to the committed files",
        rows.len()
    );
    assert_eq!(marked, rows.len());
}

/// The production decimation probe configuration, read from the registry (the
/// same construction as `plinth-pipeline`'s `registered_decimation_budget`;
/// `plinth-orient` cannot depend on the pipeline).
fn registered_decimation_budget() -> DecimationBudget {
    let placeholders = m0_constants().expect("registry").placeholders();
    let probe = ProbeConfig::new(
        placeholders.decimation_probe_grid_pitch_mm().diagnostic().get(),
        usize::try_from(placeholders.decimation_probe_max_grid_bytes().diagnostic().get())
            .map_err(|_| BudgetConfigError::InvalidGridBytes { value: 0 })
            .expect("registered grid byte count fits a usize"),
        placeholders.decimation_probe_nearest_upper_bound_mm().diagnostic().get(),
    );
    let budget = DecimationBudget::ratified_default(probe)
        .expect("the ratified probe config builds a budget");
    budget.validate_for_execution().expect("the registered budget is executable");
    budget
}

/// The mesh production prints for a MARKED source: the executor's load, the
/// repair pre-pass, the registered decimation, then `orient_as_drawn` with the
/// probe's reason (`orient_model` in `crates/plinth-pipeline/src/prepare.rs`).
fn as_printed(path: &Path) -> Mesh {
    let name = path.display().to_string();
    let reason = probe_source(path)
        .reason
        .unwrap_or_else(|| panic!("{name}: not marked as drawn"));
    let loaded = plinth_mesh::load_binary_stl(path, plinth_mesh::WeldPolicy::None)
        .unwrap_or_else(|error| panic!("{name}: {error}"));
    let (repaired, _) = plinth_mesh::repair_mesh(&loaded).into_parts();
    let decimated = plinth_decimate::run(&repaired, registered_decimation_budget())
        .unwrap_or_else(|error| panic!("{name}: decimation must run: {error}"))
        .into_decimated_model()
        .unwrap_or_else(|scale| panic!("{name}: scale sanity must pass: {scale:?}"));
    let oriented = orient_as_drawn(decimated, reason)
        .unwrap_or_else(|error| panic!("{name}: the as-drawn pose must not refuse: {error}"));
    assert_eq!(
        oriented.history().orientation().selection,
        BaselineSelection::Identity,
        "{name}: a marked model is used at identity"
    );
    oriented.mesh().clone()
}

/// The source as authored, unrotated: what "drawn" means for a file that is
/// not marked.
fn as_authored(path: &Path) -> Mesh {
    plinth_mesh::load_binary_stl(path, plinth_mesh::WeldPolicy::None)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The horizontal faces at one z extreme of a mesh: facets whose three
/// vertices all lie within `FLAT_EPS_MM` of `z`. Their total area, and whether
/// they cover the XY centre of the mesh's bounding box (a table top does, four
/// leg feet do not).
#[derive(Debug)]
struct Flat {
    facets: usize,
    area_mm2: f64,
    covers_centre: bool,
}

const FLAT_EPS_MM: f64 = 1.0e-4;

fn flat_at(mesh: &Mesh, top: bool) -> Flat {
    let verts = mesh.verts();
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for vertex in verts {
        for axis in 0..3 {
            lo[axis] = lo[axis].min(vertex[axis]);
            hi[axis] = hi[axis].max(vertex[axis]);
        }
    }
    let z = if top { hi[2] } else { lo[2] };
    let centre = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
    let mut flat = Flat {
        facets: 0,
        area_mm2: 0.0,
        covers_centre: false,
    };
    for tri in mesh.tris() {
        let [a, b, c] = tri.map(|index| verts[usize::try_from(index).expect("index fits")]);
        if [a, b, c].iter().any(|vertex| (vertex[2] - z).abs() > FLAT_EPS_MM) {
            continue;
        }
        let cross = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
        flat.facets += 1;
        flat.area_mm2 += cross.abs() / 2.0;
        let side = |p: [f64; 3], q: [f64; 3]| {
            (q[0] - p[0]) * (centre[1] - p[1]) - (q[1] - p[1]) * (centre[0] - p[0])
        };
        let (s0, s1, s2) = (side(a, b), side(b, c), side(c, a));
        if (s0 >= 0.0 && s1 >= 0.0 && s2 >= 0.0) || (s0 <= 0.0 && s1 <= 0.0 && s2 <= 0.0) {
            flat.covers_centre = true;
        }
    }
    flat
}

fn stands_on_its_legs(mesh: &Mesh) -> (bool, Flat, Flat) {
    let (bottom, top) = (flat_at(mesh, false), flat_at(mesh, true));
    // Four separate feet at the bottom, the whole top at the top.
    let legs_down = bottom.facets > 0
        && !bottom.covers_centre
        && top.covers_centre
        && top.area_mm2 > bottom.area_mm2;
    (legs_down, bottom, top)
}

fn blade_points_up(mesh: &Mesh) -> (bool, Flat, Flat) {
    let (bottom, top) = (flat_at(mesh, false), flat_at(mesh, true));
    // The slab's whole face is the lowest thing; the blade, a sheet raked 5 deg
    // off the slab, rises above the slab and ends in no face as large.
    let blade_up = bottom.covers_centre && bottom.area_mm2 > top.area_mm2 && !top.covers_centre;
    (blade_up, bottom, top)
}

#[test]
fn the_overhang_table_is_drawn_legs_down_and_the_raking_blade_blade_up() {
    let rows = rows();
    let row = |stem: &str| {
        rows.iter()
            .find(|row| row.stem == stem)
            .unwrap_or_else(|| panic!("{stem}: no sources.tsv row"))
    };

    // make_overhang_table: the owner wants LEGS DOWN (2026-10-02); the dense
    // search turns it legs up.
    let table = row("make_overhang_table");
    let (legs_down, bottom, top) = stands_on_its_legs(&as_printed(&committed(&table.stem)));
    println!("make_overhang_table as printed: bottom {bottom:?}; top {top:?}");
    assert!(
        legs_down,
        "make_overhang_table must be used as drawn LEGS DOWN: bottom {bottom:?}, top {top:?}"
    );
    let (authored_legs_down, a_bottom, a_top) = stands_on_its_legs(&as_authored(&table.source));
    println!("make_overhang_table authored: bottom {a_bottom:?}; top {a_top:?}");
    assert_eq!(
        table.turn == Turn::None,
        authored_legs_down,
        "the table is copied as is exactly when its source is drawn legs down"
    );

    // make_raking_blade: the owner wants BLADE UP (verdict 2026-09-03).
    let blade = row("make_raking_blade");
    let (blade_up, bottom, top) = blade_points_up(&as_printed(&committed(&blade.stem)));
    println!("make_raking_blade as printed: bottom {bottom:?}; top {top:?}");
    assert!(
        blade_up,
        "make_raking_blade must be used as drawn BLADE UP: bottom {bottom:?}, top {top:?}"
    );
    let (authored_blade_up, a_bottom, a_top) = blade_points_up(&as_authored(&blade.source));
    println!("make_raking_blade authored: bottom {a_bottom:?}; top {a_top:?}");
    assert!(
        !authored_blade_up,
        "the provisioned raking blade is believed drawn blade DOWN, which is why its copy is \
         turned over: bottom {a_bottom:?}, top {a_top:?}"
    );
    assert_eq!(blade.turn, Turn::X180, "a blade-down source must be turned over");
}
