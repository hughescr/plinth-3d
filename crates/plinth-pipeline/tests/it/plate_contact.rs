//! **Every plate-rooted support stands ON the plate** — the production
//! slice-sweep route, end to end, read back from the bytes on disk.
//!
//! # The defect this pins (owner, ChituBox, 2026-09-28)
//!
//! The owner opened the emitted files and saw the support bases FLOAT above
//! the build plate. The diagnosis (`plinth-evidence/slice-sweep-2026-09-02/
//! field-2026-09-28/quills.md`, "Not established", and the I5 note in its §3)
//! measured why: every root flare stopped at `plate_z + CHITUBOX_RAFT_HEIGHT_MM`
//! (0.2 mm, four layers), the TOP of a raft — and the slice-sweep path emits
//! no raft (`docs/plans/slice-sweep-core-2026-09-02.md` §14 OR-3: "v1 emits a
//! `RootFlare` per plate landing and no raft"). The legacy lowering's raft
//! datum was carried over without the raft it stands on, so each root's first
//! cured layer was four layers up, attached to nothing.
//!
//! # The rule
//!
//! A plate-rooted member's lowest solid IS its contact with the plate, so it
//! must reach the plate — `z = plate` in the file's own frame — with its full
//! flare footprint. A support that ends above the plate cures its first layer
//! against the vat film, not against the plate.
//!
//! # How this reads the file
//!
//! Only the bytes. The plate is located by the file's own frame relation —
//! `model_bbox.min.z - z_lift`, the relation `docs/specs/audit-contract.md`
//! §1.5 measured and `tools/plinth-audit` uses (`context.rs`,
//! `resolve_plate`) — never by asking the producer where it put it. A
//! `FLARE` record (type 4) is by format definition a plate root
//! (`docs/specs/chitubox-format.md`), so every one is graded; a model with
//! none would grade nothing, and that is asserted against too.
//!
//! The fixture is the PROCEDURAL `Round 25` (`plinth_mesh::make_recessed_disc`
//! at the dimensions `crates/plinth-sweep/tests/it/round_bases_ratchet.rs`
//! derives and measures against the real STL), written out as a real binary
//! `.stl`, so this runs in a fresh clone with no provisioned corpus.

use std::path::PathBuf;

use plinth_pipeline::{
    prepare, registered_decimation_budget, registered_support_request, ModelPreparationOutcome,
    OrientationPolicy, OutputProfile, PrepareRequest,
};

/// The owner's stated resolution for "touches the plate": 1e-4 mm, two
/// orders under one 0.05 mm layer and well above the f32 resolution of a
/// coordinate a few tens of mm from the origin.
const PLATE_CONTACT_TOLERANCE_MM: f64 = 1e-4;

/// `Round 25`'s parametric dimensions, verbatim from
/// `round_bases_ratchet.rs`'s `round_25()` and its `HEIGHT_MM` /
/// `RECESS_DEPTH_MM`.
const ROUND_25_BOTTOM_RADIUS_MM: f64 = 11.5;
const ROUND_25_TOP_RADIUS_MM: f64 = 12.5;
const ROUND_25_HEIGHT_MM: f64 = 3.5;
const ROUND_25_RECESS_DEPTH_MM: f64 = 2.5;
const ROUND_25_RECESS_RADIUS_MM: f64 = 10.0;
const ROUND_25_SEGMENTS: usize = 96;

struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "plinth-plate-contact-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("in")).expect("scratch input folder");
        Self(path)
    }

    fn input(&self) -> PathBuf {
        self.0.join("in")
    }

    fn output(&self) -> PathBuf {
        self.0.join("out")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Write the procedural `Round 25` as the binary `.stl` a user would hand in.
/// Coordinates are snapped to f32 first because that is exactly what an
/// `.stl` holds (`prepare_folder.rs`'s `place_procedural` states why).
#[expect(
    clippy::cast_possible_truncation,
    reason = "test-local f32 snap of a bounded procedural fixture, the narrowing an .stl performs"
)]
fn place_round_25(scratch: &Scratch, stem: &str) {
    let mesh = plinth_mesh::make_recessed_disc(
        ROUND_25_BOTTOM_RADIUS_MM,
        ROUND_25_TOP_RADIUS_MM,
        ROUND_25_HEIGHT_MM,
        ROUND_25_RECESS_DEPTH_MM,
        ROUND_25_RECESS_RADIUS_MM,
        ROUND_25_SEGMENTS,
    )
    .expect("a valid Round 25 profile");
    let verts: Vec<[f64; 3]> = mesh
        .verts()
        .iter()
        .map(|vertex| vertex.map(|value| f64::from(value as f32)))
        .collect();
    let snapped = plinth_mesh::Mesh::new(verts, mesh.tris().to_vec())
        .expect("an f32-snapped procedural fixture is still a valid mesh");
    plinth_mesh::write_binary_stl(scratch.input().join(format!("{stem}.stl")), &snapped)
        .expect("write the Round 25 fixture");
}

/// The production request the CLI builds for
/// `plinth prepare --output saturn4-ultra --orient dense --placement sweep`.
fn sweep_request(scratch: &Scratch) -> PrepareRequest {
    let printer = OutputProfile::Saturn4Ultra
        .resolve()
        .expect("M0 Saturn profile")
        .clone();
    PrepareRequest {
        input_models_dir: scratch.input(),
        output_dir: scratch.output(),
        orientation: OrientationPolicy::DenseSweep,
        placement: plinth_pipeline::Placement::Sweep,
        output_profile: OutputProfile::Saturn4Ultra,
        decimation_budget: registered_decimation_budget().expect("registered budget"),
        hollow: plinth_hollow::HollowRequest::with_registered_wall_default(printer.clone())
            .expect("registered wall"),
        support: registered_support_request(&printer).expect("registered lift"),
        no_cache: true,
        stop_after: None,
    }
}

/// One FLARE record's contact with the plate, as the file holds it.
struct FlareFoot {
    record: usize,
    /// The record's own lower end point, file frame.
    lower_end_z_mm: f64,
    /// The lowest vertex of the record's emitted triangle soup.
    soup_min_z_mm: f64,
    /// The largest in-plane distance from the lower end point to any soup
    /// vertex lying ON the plate plane: the radius of the ring that touches.
    /// Zero when no vertex lies on the plate at all.
    ring_on_plate_radius_mm: f64,
    /// The radius the record declares at its lower end.
    declared_foot_radius_mm: f64,
}

#[test]
fn every_plate_root_of_round_25_stands_on_the_plate_with_its_full_flare_footprint() {
    let scratch = Scratch::new("round25");
    place_round_25(&scratch, "Round_25");
    let report = prepare(sweep_request(&scratch)).expect("the sweep route emits and warns");
    let entry = report
        .models
        .iter()
        .find(|entry| entry.model.as_str() == "Round_25")
        .expect("Round_25 is in the report");
    let ModelPreparationOutcome::Artifact { emit, .. } = &entry.outcome else {
        panic!(
            "Round 25 must reach an artifact; it reported {:?}",
            entry.outcome
        );
    };

    let project =
        chitubox_io::Project::open(&emit.artifact_path).expect("the artifact must reopen");
    let info = project.model(0).expect("model 0");
    let model_min_z_mm = project
        .model_triangles(0)
        .expect("model triangles")
        .chunks_exact(3)
        .map(|vertex| f64::from(vertex[2]))
        .fold(f64::INFINITY, f64::min);
    // The file's own frame relation (audit-contract.md §1.5).
    let plate_z_mm = model_min_z_mm - f64::from(info.placement.z_lift);
    let records = project.supports(0).expect("support records");

    let feet: Vec<FlareFoot> = records
        .iter()
        .enumerate()
        .filter(|(_, record)| record.segment_type == chitubox_io::SegmentType::FLARE)
        .map(|(index, record)| {
            let (lower, declared) = if record.start[2] <= record.end[2] {
                (record.start, record.r_start)
            } else {
                (record.end, record.r_end)
            };
            let soup_min_z_mm = record
                .mesh
                .xyz
                .chunks_exact(3)
                .map(|vertex| f64::from(vertex[2]))
                .fold(f64::INFINITY, f64::min);
            let ring_on_plate_radius_mm = record
                .mesh
                .xyz
                .chunks_exact(3)
                .filter(|vertex| {
                    (f64::from(vertex[2]) - plate_z_mm).abs() <= PLATE_CONTACT_TOLERANCE_MM
                })
                .map(|vertex| {
                    (f64::from(vertex[0]) - f64::from(lower[0]))
                        .hypot(f64::from(vertex[1]) - f64::from(lower[1]))
                })
                .fold(0.0, f64::max);
            FlareFoot {
                record: index,
                lower_end_z_mm: f64::from(lower[2]),
                soup_min_z_mm,
                ring_on_plate_radius_mm,
                declared_foot_radius_mm: f64::from(declared),
            }
        })
        .collect();

    // PATH HIT: the plate-root chain fired. A disc that grew no flare would
    // pass every clause below vacuously.
    assert!(
        !feet.is_empty(),
        "Round 25 emitted no FLARE record, so no plate root was graded"
    );

    let floating: Vec<String> = feet
        .iter()
        .filter(|foot| {
            (foot.lower_end_z_mm - plate_z_mm).abs() > PLATE_CONTACT_TOLERANCE_MM
                || (foot.soup_min_z_mm - plate_z_mm).abs() > PLATE_CONTACT_TOLERANCE_MM
        })
        .map(|foot| {
            format!(
                "record {} lower end {:+.4} mm, soup min {:+.4} mm above the plate",
                foot.record,
                foot.lower_end_z_mm - plate_z_mm,
                foot.soup_min_z_mm - plate_z_mm
            )
        })
        .collect();
    let narrow: Vec<String> = feet
        .iter()
        .filter(|foot| (foot.ring_on_plate_radius_mm - foot.declared_foot_radius_mm).abs() > 1e-3)
        .map(|foot| {
            format!(
                "record {} touches the plate over radius {:.4} mm, declares {:.4} mm",
                foot.record, foot.ring_on_plate_radius_mm, foot.declared_foot_radius_mm
            )
        })
        .collect();
    println!(
        "Round 25: {} plate roots graded; plate z {plate_z_mm:+.6} mm (file frame); {} floating, {} narrow",
        feet.len(),
        floating.len(),
        narrow.len()
    );
    assert!(
        floating.is_empty(),
        "{} of {} plate roots do not reach the plate (it is at {plate_z_mm:+.6} mm): {floating:?}",
        floating.len(),
        feet.len()
    );
    assert!(
        narrow.is_empty(),
        "{} of {} plate roots touch the plate with less than their full flare footprint: {narrow:?}",
        narrow.len(),
        feet.len()
    );
}
