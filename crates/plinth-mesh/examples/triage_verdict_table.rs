//! The triage-verdict table: the hard prerequisite for `docs/specs/
//! geometry-canon.md` §1 OP-2 / plan item C5 (ray vs triangle).
//!
//! C5 changes whether a component is admitted, and a refused model emits no
//! file, so the byte audit's denominator silently shrinks and it cannot say
//! why (`docs/plans/geometry-consolidation.md` §4). The instrument that CAN
//! say why is this one: per corpus model, the triage verdict, the shard/cavity
//! summary, a digest of the mesh triage hands downstream, and — because a
//! recorded project failure was a byte-identity proof over a model that never
//! fired the changed path — the measured count of ray/triangle face tests that
//! actually ran.
//!
//! Like `corpus_gate`, this is an example and not a test: the corpus is
//! locally provisioned and `cargo test --workspace` must stay a clean-checkout
//! gate.
//!
//! Usage: `cargo run --release -p plinth-mesh --example triage_verdict_table`
//! writes the table to stdout. Redirect it to a file, capture it before the
//! change, and diff.
//!
//! The run FAILS rather than reports if it processed fewer models than the
//! manifest names: a verdict table that discovered zero models must not look
//! like a table of zero changes (the xargs-processed-zero-files rule).

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use plinth_mesh::{
    containment_ray_counters, load_binary_stl, reset_containment_ray_counters, triage_components,
    Mesh, TriageError, TriageTopologyAdmission, TriageTopologyLimits, WeldPolicy,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct Manifest {
    models: Vec<ManifestModel>,
}

#[derive(Deserialize)]
struct ManifestModel {
    name: String,
    stl: String,
}

fn main() -> ExitCode {
    match run() {
        Ok(table) => {
            print!("{table}");
            ExitCode::SUCCESS
        }
        Err(reason) => {
            eprintln!("triage verdict table failed: {reason}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<String, String> {
    let root = workspace_root()?;
    let manifest_path = root.join("corpus/manifest.json");
    let bytes = fs::read(&manifest_path).map_err(|error| {
        format!(
            "manifest unreadable at {}: {error}",
            manifest_path.display()
        )
    })?;
    let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "manifest at {} is not typed JSON: {error}",
            manifest_path.display()
        )
    })?;
    let expected = manifest.models.len();
    if expected == 0 {
        return Err("manifest names zero models; there is nothing to tabulate".to_string());
    }

    let mut table = String::new();
    table.push_str("# Triage verdict table\n\n");
    table.push_str(
        "One row per corpus model. `verdict` is `triage_components(production limits, \
         ExactBits)`; `digest` is SHA-256 over the triaged mesh's vertex and triangle bits \
         (`-` when triage refused). `ray_tests`/`hits`/`misses`/`ambig`/`near_par` are measured \
         counts from the containment ray caster, and `classifiers` is how many nested-component \
         containment queries ran at all: a row with `classifiers=0` did not exercise the \
         ray/triangle path, so its unchanged verdict is NOT evidence about C5.\n\n",
    );
    table.push_str(
        "`near_par` is the near-parallel column: ray/triangle tests the KERNEL could not \
         decide (`0 < |det| <= DET_EPS`), which before \
         `docs/specs/geometry-adjudication.md` were reported as definite misses and dropped a \
         real interior crossing out of the parity count. It is DISJOINT from `ambig`, which \
         counts a different uncertainty — a crossing that exists but whose sign triage cannot \
         sign. A nonzero `near_par` is the measured refusal rate the adjudication exists to \
         produce, not a value to tune away; a zero one says the corpus never entered the band, \
         which is a result and not an omission.\n\n",
    );
    table.push_str(
        "| model | faces | verdict | components | main | stripped_comp | stripped_faces | \
         digest | classifiers | rays | ray_tests | hits | misses | ambig | near_par | on_tri |\n",
    );
    table.push_str("|---|--:|---|--:|--:|--:|--:|---|--:|--:|--:|--:|--:|--:|--:|--:|\n");

    let mut processed = 0_usize;
    for model in &manifest.models {
        let path = root.join(&model.stl);
        let mesh = load_binary_stl(&path, WeldPolicy::None)
            .map_err(|error| format!("could not load {}: {error}", path.display()))?;
        let faces = mesh.tris().len();

        reset_containment_ray_counters();
        let verdict = triage_one(&mesh);
        let counters = containment_ray_counters();

        let (verdict_text, components, main, stripped_components, stripped_faces, digest) =
            match &verdict {
                Ok(triaged) => {
                    let summary = triaged.summary();
                    (
                        "Ok".to_string(),
                        summary.component_count().to_string(),
                        summary.main_component().to_string(),
                        summary.stripped_components().len().to_string(),
                        summary.stripped_faces().len().to_string(),
                        mesh_digest(triaged.mesh()),
                    )
                }
                Err(error) => (
                    refusal_label(error),
                    "-".to_string(),
                    "-".to_string(),
                    "-".to_string(),
                    "-".to_string(),
                    "-".to_string(),
                ),
            };

        writeln!(
            table,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            model.name,
            faces,
            verdict_text,
            components,
            main,
            stripped_components,
            stripped_faces,
            digest,
            counters.classifiers_built,
            counters.rays_cast,
            counters.face_tests,
            counters.face_hits,
            counters.face_misses,
            counters.face_ambiguous,
            counters.face_near_parallel,
            counters.point_on_triangle_hits,
        )
        .map_err(|error| format!("could not format row for {}: {error}", model.name))?;
        processed += 1;
    }

    // Gate on the processed count, never on the exit code of the loop.
    if processed != expected {
        return Err(format!(
            "processed {processed} models but the manifest names {expected}; a partial table is \
             not a table"
        ));
    }
    writeln!(table, "\nrows: {processed} (manifest names {expected})")
        .map_err(|error| format!("could not write the row-count gate: {error}"))?;

    let (probe_table, fired) = nested_probe_table(&root, &manifest)?;
    table.push_str(&probe_table);

    // The load-bearing gate. Measured on this corpus, EVERY plain row above
    // has `classifiers = 0`: no corpus model contains a nested watertight
    // component, so triage never reaches the ray caster and the plain table's
    // "nothing moved" is a true negative that says nothing about C5. The probe
    // section exists to make the table evidence rather than decoration, and a
    // run in which the probes ALSO fired nothing is a broken instrument
    // reporting success — the failure mode this project keeps reproducing.
    if fired == 0 {
        return Err(
            "no nested probe reached the ray/triangle path (0 face tests across every probe \
             row); this table would be evidence of nothing"
                .to_string(),
        );
    }
    writeln!(table, "\nprobe ray/triangle face tests, all rows: {fired}")
        .map_err(|error| format!("could not write the fired-count gate: {error}"))?;
    Ok(table)
}

/// Force the containment ray caster onto REAL corpus geometry.
///
/// No corpus model has a nested watertight component, so none of them exercises
/// `ContainmentClassifier` on its own — measured, and the reason this section
/// exists. Nesting a 2 mm box at each model's bounding-box centre creates
/// exactly the sealed-cavity shape triage exists to refuse, against the model's
/// own triangles at the model's own coordinates. The box is 2 mm across
/// (`hollow_shard_max_dim_mm` is 1.0) and shares no vertex with the model, so
/// it cannot be dismissed as a shard and must reach the containment check.
///
/// The verdict of a probe row is not itself interesting — the interesting
/// columns are `ray_tests` and the hit/miss split, which are what C5 changes.
fn nested_probe_table(root: &Path, manifest: &Manifest) -> Result<(String, u64), String> {
    let mut table = String::new();
    table.push_str("\n## Nested-shell probes on real corpus geometry\n\n");
    table.push_str(
        "A 2 mm box nested at each model's bounding-box centre, so the ray/triangle path runs \
         against that model's own triangles. `ray_tests` is the measured number of \
         ray-vs-triangle evaluations; `hits`/`misses`/`ambig`/`near_par` are their outcomes, and \
         they partition it — every test lands in exactly one.\n\n",
    );
    table.push_str(
        "| model | verdict | classifiers | rays | ray_tests | hits | misses | ambig | near_par | \
         on_tri |\n",
    );
    table.push_str("|---|---|--:|--:|--:|--:|--:|--:|--:|--:|\n");

    let mut fired = 0_u64;
    let mut near_parallel = 0_u64;
    let mut processed = 0_usize;
    for model in &manifest.models {
        let path = root.join(&model.stl);
        let mesh = load_binary_stl(&path, WeldPolicy::None)
            .map_err(|error| format!("could not load {}: {error}", path.display()))?;
        let centre = bbox_centre(&mesh);
        let probe = plinth_mesh::make_box([2.0, 2.0, 2.0], centre)
            .map_err(|error| format!("probe box for {}: {error}", model.name))?;
        let joined = join(&mesh, &probe);

        reset_containment_ray_counters();
        let verdict = triage_one(&joined);
        let counters = containment_ray_counters();
        fired += counters.face_tests;
        near_parallel += counters.face_near_parallel;

        writeln!(
            table,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            model.name,
            match &verdict {
                Ok(_) => "Ok".to_string(),
                Err(error) => refusal_label(error),
            },
            counters.classifiers_built,
            counters.rays_cast,
            counters.face_tests,
            counters.face_hits,
            counters.face_misses,
            counters.face_ambiguous,
            counters.face_near_parallel,
            counters.point_on_triangle_hits,
        )
        .map_err(|error| format!("could not format probe row for {}: {error}", model.name))?;
        processed += 1;
    }
    if processed != manifest.models.len() {
        return Err(format!(
            "probed {processed} models but the manifest names {}",
            manifest.models.len()
        ));
    }
    // Stated whether or not it fired. A zero here is a measured result — the
    // corpus probes never entered the undecidable band — and printing it is
    // what stops a later reader mistaking a missing line for a clean run.
    writeln!(
        table,
        "\nprobe rows: {processed}; probe near-parallel ray/triangle tests, all rows: \
         {near_parallel}"
    )
    .map_err(|error| format!("could not write the probe row count: {error}"))?;
    Ok((table, fired))
}

fn bbox_centre(mesh: &Mesh) -> [f64; 3] {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for vertex in mesh.verts() {
        for axis in 0..3 {
            lo[axis] = lo[axis].min(vertex[axis]);
            hi[axis] = hi[axis].max(vertex[axis]);
        }
    }
    [
        (lo[0] + hi[0]) / 2.0,
        (lo[1] + hi[1]) / 2.0,
        (lo[2] + hi[2]) / 2.0,
    ]
}

fn join(left: &Mesh, right: &Mesh) -> Mesh {
    let offset = u32::try_from(left.verts().len()).expect("corpus meshes index within u32");
    let mut verts = left.verts().to_vec();
    verts.extend_from_slice(right.verts());
    let mut tris = left.tris().to_vec();
    tris.extend(right.tris().iter().map(|triangle| {
        [
            triangle[0] + offset,
            triangle[1] + offset,
            triangle[2] + offset,
        ]
    }));
    Mesh::new(verts, tris).expect("joined probe mesh")
}

fn triage_one(mesh: &Mesh) -> Result<plinth_mesh::TriagedMesh, TriageError> {
    let admission =
        TriageTopologyAdmission::from_raw_mesh(mesh, TriageTopologyLimits::production())
            .map_err(TriageError::Admission)?;
    triage_components(admission, WeldPolicy::ExactBits)
}

fn refusal_label(error: &TriageError) -> String {
    match error {
        TriageError::PreHollowed(refusal) => {
            format!("PreHollowed({})", refusal.cavity_components().len())
        }
        other => {
            let text = format!("{other:?}");
            match text.find(['(', ' ', '{']) {
                Some(cut) => text[..cut].to_string(),
                None => text,
            }
        }
    }
}

fn mesh_digest(mesh: &Mesh) -> String {
    let mut hasher = Sha256::new();
    hasher.update((mesh.verts().len() as u64).to_le_bytes());
    for vertex in mesh.verts() {
        for axis in vertex {
            hasher.update(axis.to_bits().to_le_bytes());
        }
    }
    hasher.update((mesh.tris().len() as u64).to_le_bytes());
    for triangle in mesh.tris() {
        for index in triangle {
            hasher.update(index.to_le_bytes());
        }
    }
    format!("{:x}", hasher.finalize())
}

fn workspace_root() -> Result<PathBuf, String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| format!("no workspace root above {}", manifest_dir.display()))?;
    Ok(root.to_path_buf())
}
