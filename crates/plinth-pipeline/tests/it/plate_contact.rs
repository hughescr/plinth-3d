//! **Every plate-rooted support stands ON the plate** — the production
//! slice-sweep route, end to end, read back from the bytes on disk.
//!
//! # 2026-09-29 (fix/sweep-raft): through a raft pad
//!
//! The owner added ChituBox's own raft to our Round files and it caught only
//! some feet (`plinth-evidence/.../field-2026-09-28/raft-diagnosis.md`): the
//! files carried no raft, while every ChituBox-authored file writes a pad
//! under every plate support and ends the foot ON the pad. The sweep path now
//! emits that raft (the revived legacy bottom-connected planner), so a plate
//! root stands on its pad and the PAD stands on the plate.
//!
//! The first test here, `every_plate_root_of_round_25_stands_on_the_plate_
//! with_its_full_flare_footprint` (fix/plate-gap), graded the flare's own foot
//! ON the plate with its full footprint. That was the right rule while no raft
//! was emitted; with the raft it is the wrong one by the owner's decision, and
//! it went red as designed (43 of 43 flares 0.1980 mm above the plate, i.e. on
//! their pads). It is RETIRED, not edited: its claim — the plate is reached by
//! the full footprint of what stands on it — is carried by the test below,
//! which grades each pad's bottom on the plate and each foot inside its pad.
//! The history that motivated it follows unchanged.
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

// ---------------------------------------------------------------------------
// THE RAFT (fix/sweep-raft, 2026-09-29)
// ---------------------------------------------------------------------------

/// A ChituBox-authored file with ChituBox's own bottom-connected raft: a pad
/// under every plate support, bridges between them, every foot ending on its
/// pad. It is the NATIVE ORACLE for how deep a foot reaches into its pad; the
/// test measures it rather than typing the number.
const NATIVE_BOTTOM_CONNECTED: &[u8] = include_bytes!(
    "../../../../scripts/tests/fixtures/chitubox/cylinder-supported-bottom-connected.chitubox"
);

/// Trailing slot 0 of a type-6 record: `0.0` on a pad, `1.0` on a bridge
/// (`docs/specs/chitubox-format.md`; measured on every native file).
fn raft_slot0(record: &chitubox_io::SupportRecord) -> f32 {
    f32::from_le_bytes([
        record.trailing[0],
        record.trailing[1],
        record.trailing[2],
        record.trailing[3],
    ])
}

fn soup_z_range(record: &chitubox_io::SupportRecord) -> (f64, f64) {
    record
        .mesh
        .xyz
        .chunks_exact(3)
        .map(|vertex| f64::from(vertex[2]))
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), z| {
            (lo.min(z), hi.max(z))
        })
}

/// The record's lower end point (a frustum's end nearest the plate).
fn lower_end(record: &chitubox_io::SupportRecord) -> [f32; 3] {
    if record.start[2] <= record.end[2] {
        record.start
    } else {
        record.end
    }
}

/// Andrew's monotone chain: the convex hull of a point set, counter-clockwise.
/// A GENERIC hull of the pad's own emitted vertices, so "the pad covers the
/// foot" is read from the file's triangles, not from the producer's formula.
fn convex_hull(mut points: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    points.dedup();
    if points.len() < 3 {
        return points;
    }
    let cross = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut hull: Vec<[f64; 2]> = Vec::with_capacity(points.len() * 2);
    for pass in 0..2 {
        let start = hull.len();
        let iter: Box<dyn Iterator<Item = &[f64; 2]>> = if pass == 0 {
            Box::new(points.iter())
        } else {
            Box::new(points.iter().rev())
        };
        for &point in iter {
            while hull.len() >= start + 2
                && cross(hull[hull.len() - 2], hull[hull.len() - 1], point) <= 0.0
            {
                hull.pop();
            }
            hull.push(point);
        }
        hull.pop();
    }
    hull
}

/// How far `point` lies INSIDE a counter-clockwise convex polygon: the
/// smallest signed distance to any edge line, negative when outside.
fn inside_margin(hull: &[[f64; 2]], point: [f64; 2]) -> f64 {
    (0..hull.len())
        .map(|index| {
            let a = hull[index];
            let b = hull[(index + 1) % hull.len()];
            let edge = [b[0] - a[0], b[1] - a[1]];
            let length = edge[0].hypot(edge[1]);
            (edge[0] * (point[1] - a[1]) - edge[1] * (point[0] - a[0])) / length
        })
        .fold(f64::INFINITY, f64::min)
}

/// Native foot-into-pad overlap, measured: for every FLARE of the native file,
/// its pad's top minus the flare's lowest vertex. Returns the largest.
fn native_foot_pad_overlap_mm() -> f64 {
    let project = chitubox_io::Project::from_bytes(NATIVE_BOTTOM_CONNECTED.to_vec())
        .expect("the native fixture parses");
    let records = project.supports(0).expect("native supports");
    let pads: Vec<&chitubox_io::SupportRecord> = records
        .iter()
        .filter(|r| {
            r.segment_type == chitubox_io::SegmentType::PAD_OR_BRIDGE && raft_slot0(r) == 0.0
        })
        .collect();
    let mut overlap = 0.0_f64;
    let mut graded = 0;
    for flare in records
        .iter()
        .filter(|r| r.segment_type == chitubox_io::SegmentType::FLARE)
    {
        let foot = lower_end(flare);
        let pad = pads
            .iter()
            .find(|pad| {
                (f64::from(pad.start[0]) - f64::from(foot[0]))
                    .hypot(f64::from(pad.start[1]) - f64::from(foot[1]))
                    <= PLATE_CONTACT_TOLERANCE_MM
            })
            .expect("every native flare stands on a native pad");
        overlap = overlap.max(soup_z_range(pad).1 - soup_z_range(flare).0);
        graded += 1;
    }
    assert!(graded > 0, "the native fixture has no flare to measure");
    overlap
}

/// **Every plate root of Round 25 stands on a raft pad that covers its foot**
/// (`raft-diagnosis.md` §8; owner ruling 2026-09-29).
///
/// The owner added ChituBox's own raft to our Round files and it caught only
/// some feet: our files carried NO raft, every ChituBox-authored file writes a
/// pad under every plate support with bridges between the pads, and the
/// native foot ends ON its pad. The rule, read from the bytes:
///
/// - every support whose lowest point is at the raft (every FLARE, and any
///   straight column that stands there) has exactly one pad (type 6, slot 0 =
///   0.0) centred under it, and the pad's own emitted polygon covers the
///   foot's lowest ring (owner ruling: grow the pad, never cap the foot);
/// - the pad stands on the plate and rises to the plate plus its height;
/// - the foot ends inside the band from the pad top minus the NATIVE overlap
///   (measured from a ChituBox file above) to the pad top: on the pad, not
///   through it to the plate and not above it;
/// - nothing but raft records reaches below that band;
/// - the pads and bridges (slot 0 = 1.0) form ONE connected graph;
/// - every type-6 record writes `z = 0.0`, as every native file does.
///
/// Red before (develop `c33dc8e`): 0 pads, 44 defects. Sabotage, run at the
/// change: the foot put back on the plate while pads are emitted — 43 feet
/// "through the pad"; every pad exactly as wide as its foot (the polygon
/// margin removed) — the 2 grown pads' feet poke ~0.015 mm outside their pad.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one end-to-end read of the file, graded clause by clause"
)]
fn every_plate_root_of_round_25_stands_on_a_raft_pad_that_covers_its_foot() {
    let native_overlap_mm = native_foot_pad_overlap_mm();
    let scratch = Scratch::new("round25-raft");
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
    let plate_z_mm = model_min_z_mm - f64::from(info.placement.z_lift);
    let records = project.supports(0).expect("support records");

    let is_raft =
        |r: &chitubox_io::SupportRecord| r.segment_type == chitubox_io::SegmentType::PAD_OR_BRIDGE;
    let pads: Vec<usize> = (0..records.len())
        .filter(|&i| is_raft(&records[i]) && raft_slot0(&records[i]) == 0.0)
        .collect();
    let bridges: Vec<usize> = (0..records.len())
        .filter(|&i| is_raft(&records[i]) && raft_slot0(&records[i]).to_bits() == 1.0_f32.to_bits())
        .collect();
    let flares = records
        .iter()
        .filter(|r| r.segment_type == chitubox_io::SegmentType::FLARE)
        .count();
    println!(
        "Round 25 raft: {} pads, {} bridges, {} flares; plate {plate_z_mm:+.6} mm; native foot-pad overlap {native_overlap_mm:.6} mm",
        pads.len(),
        bridges.len(),
        flares
    );
    // PATH HIT: the plate-root chain fired at all.
    assert!(flares > 0, "Round 25 emitted no FLARE record");

    let mut defects: Vec<String> = Vec::new();

    // Every type-6 record is in the native frame: z = 0.0.
    for &i in pads.iter().chain(&bridges) {
        let r = &records[i];
        if r.start[2].to_bits() != 0.0_f32.to_bits() || r.end[2].to_bits() != 0.0_f32.to_bits() {
            defects.push(format!(
                "raft record {i} writes z {} / {}, native files write 0.0",
                r.start[2], r.end[2]
            ));
        }
    }
    if pads.len() + bridges.len() != records.iter().filter(|r| is_raft(r)).count() {
        defects
            .push("a type-6 record is neither a pad (slot 0 = 0) nor a bridge (slot 0 = 1)".into());
    }

    // Every pad: on the plate, up to plate + its own height slot.
    let mut pad_top_mm = Vec::with_capacity(pads.len());
    for &i in &pads {
        let (lo, hi) = soup_z_range(&records[i]);
        if (lo - plate_z_mm).abs() > PLATE_CONTACT_TOLERANCE_MM {
            defects.push(format!(
                "pad {i} bottom is {:+.5} mm off the plate",
                lo - plate_z_mm
            ));
        }
        if (hi - lo - f64::from(records[i].r_end)).abs() > PLATE_CONTACT_TOLERANCE_MM {
            defects.push(format!(
                "pad {i} is {:.5} mm tall, its height slot says {}",
                hi - lo,
                records[i].r_end
            ));
        }
        pad_top_mm.push(hi);
    }
    let raft_top_mm = pad_top_mm.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let band_low_mm = raft_top_mm - native_overlap_mm - PLATE_CONTACT_TOLERANCE_MM;

    // Every support that stands at the raft: one covering pad, foot in band.
    let mut graded = 0;
    let mut pads_used = vec![0_usize; pads.len()];
    for (i, record) in records.iter().enumerate() {
        if is_raft(record) {
            continue;
        }
        let (lo, _) = soup_z_range(record);
        let is_foot = record.segment_type == chitubox_io::SegmentType::FLARE
            || lo <= raft_top_mm + PLATE_CONTACT_TOLERANCE_MM;
        graded += usize::from(is_foot);
        if lo < band_low_mm {
            defects.push(format!(
                "record {i} (type {:?}) reaches {:.5} mm below the raft top, through the pad",
                record.segment_type,
                raft_top_mm - lo
            ));
            continue;
        }
        if !is_foot {
            continue;
        }
        let foot = lower_end(record);
        let under: Vec<usize> = (0..pads.len())
            .filter(|&p| {
                let pad = &records[pads[p]];
                (f64::from(pad.start[0]) - f64::from(foot[0]))
                    .hypot(f64::from(pad.start[1]) - f64::from(foot[1]))
                    <= PLATE_CONTACT_TOLERANCE_MM
            })
            .collect();
        let [p] = under.as_slice() else {
            defects.push(format!(
                "record {i} stands on {} pads centred under it, not one",
                under.len()
            ));
            continue;
        };
        pads_used[*p] += 1;
        let top = pad_top_mm[*p];
        if lo > top + PLATE_CONTACT_TOLERANCE_MM
            || lo < top - native_overlap_mm - PLATE_CONTACT_TOLERANCE_MM
        {
            defects.push(format!(
                "record {i} foot ends {:+.5} mm from its pad top; the band is [-{native_overlap_mm:.4}, 0]",
                lo - top
            ));
        }
        // The foot's lowest ring, inside the pad's own emitted polygon.
        let hull = convex_hull(
            records[pads[*p]]
                .mesh
                .xyz
                .chunks_exact(3)
                .map(|v| [f64::from(v[0]), f64::from(v[1])])
                .collect(),
        );
        let worst = record
            .mesh
            .xyz
            .chunks_exact(3)
            .filter(|v| f64::from(v[2]) <= lo + PLATE_CONTACT_TOLERANCE_MM)
            .map(|v| inside_margin(&hull, [f64::from(v[0]), f64::from(v[1])]))
            .fold(f64::INFINITY, f64::min);
        if worst < -1e-5 {
            defects.push(format!(
                "record {i} foot ring pokes {:.5} mm outside its pad (foot r {:.4})",
                -worst,
                if record.start[2] <= record.end[2] {
                    record.r_start
                } else {
                    record.r_end
                }
            ));
        }
    }
    if graded < flares {
        defects.push(format!(
            "fewer feet graded ({graded}) than flares ({flares})"
        ));
    }
    if let Some(unused) = pads_used.iter().position(|&n| n == 0) {
        defects.push(format!("pad {} carries no foot", pads[unused]));
    }

    // Pads and bridges: one connected graph.
    let find_pad = |xy: [f32; 3]| {
        (0..pads.len()).find(|&p| {
            let pad = &records[pads[p]];
            (f64::from(pad.start[0]) - f64::from(xy[0]))
                .hypot(f64::from(pad.start[1]) - f64::from(xy[1]))
                <= PLATE_CONTACT_TOLERANCE_MM
        })
    };
    let mut parent: Vec<usize> = (0..pads.len()).collect();
    fn root(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    for &b in &bridges {
        match (find_pad(records[b].start), find_pad(records[b].end)) {
            (Some(a), Some(c)) => {
                let (ra, rc) = (root(&mut parent, a), root(&mut parent, c));
                parent[ra] = rc;
            }
            _ => defects.push(format!("bridge {b} does not end on two pads")),
        }
    }
    let components = (0..pads.len())
        .filter(|&p| root(&mut parent, p) == p)
        .count();
    if components != 1 {
        defects.push(format!("the raft is {components} separate pieces, not one"));
    }

    println!(
        "Round 25 raft: {graded} feet graded on {} pads; {} defects",
        pads.len(),
        defects.len()
    );
    assert!(
        defects.is_empty(),
        "{} raft defects: {defects:#?}",
        defects.len()
    );
}
