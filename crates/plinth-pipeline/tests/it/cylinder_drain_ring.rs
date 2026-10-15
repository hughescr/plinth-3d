//! **The Cylinder's drain ring: no pile of overlapping plate posts**
//! (fix/stranded-seat-branch, 2026-10-02), graded from the bytes on disk.
//!
//! # The defect this pins
//!
//! The owner opened `for-craig-v11/Cylinder` in ChituBox and found the posts
//! by the drain overlapping. Measured on that file
//! (`plinth-evidence/slice-sweep-2026-09-02/field-2026-09-28/
//! v11-feedback-diagnosis.md`, item 1): 12 plate posts on a ring 2.85-2.87 mm
//! from the drain axis, 0.54-1.1 mm apart; 16 overlapping root pairs, 14 of
//! them on that ring; 129 census cells under three or more roots. The cause was
//! the stranded seating standing a new plate post on every layer the keep-out's
//! demand came back on, each beside the last.
//!
//! # The rule
//!
//! A stranded-seat member whose plate root laps a standing root lands on that
//! root's member as a branch (`plinth_sweep::stranded_branch`).
//!
//! # What is graded, and how
//!
//! Only the file, the way the diagnosis's own probe reads it (`tools/roots.ts`,
//! which reproduced the run log's 16 pairs exactly). Records are grouped by
//! member the way the file chains them: a TIP (type 7) opens a member; a TAPER
//! (type 1) right after it is that member's shaft, a TAPER followed by a
//! JOINER or FLARE opens a mast, any other TAPER is a brace; a JOINER (type 3)
//! and a FLARE (type 4) continue the member before them. A member with a FLARE
//! stands on the plate. Its root is its shaft's radius at the lowest shaft
//! end, and its disc is that root times the registered `FLARE_R_FACTOR`, the
//! census's crowding disc. Two discs overlap when their centres are closer
//! than the sum of their radii; a cell is stacked when three or more discs
//! cover its centre, on a 0.05 mm raster.
//!
//! * **No cell under three or more roots.** 129 on `for-craig-v11`.
//! * **No overlapping root pair outside the pinned five.** 16 on
//!   `for-craig-v11`. Every pair the file shows must be one of
//!   [`PINNED_PAIRS`], named by the two root centres (file coordinates, mm;
//!   the drain axis is at x = y = 0). A pair anywhere else is a new pile-up and
//!   fails, whatever the count; a pinned pair that goes away is an improvement
//!   and passes.
//!
//!   Re-pinned at the round-16 milestone (2026-10-05, develop `be4c179`),
//!   from "at most four pairs" to the five named below. The rule change that
//!   moved it is the cylinder-bracing lane's round 2 (`6b0d238`: the
//!   hidden-cavity map holds whole cells only, and a straight descent wholly
//!   in a hidden cavity is not refused at the final gap). Measured on the
//!   lane's own runs: 4 pairs at `9577dd0` (round 1, = `for-craig-v15`),
//!   5 at `6b0d238` and at `2019f39`. The fifth pair is (-6.687, 0.763) and
//!   (-4.737, 1.663), 6.73 and 5.02 mm from the drain axis, off the ring: the
//!   pair this pin's first version already named as one of "two pairs off the
//!   ring (5-6.7 mm from the axis) that already stood in round 10, before
//!   stranded seating existed". It came back because the post at
//!   (-4.737, 1.663), a short straight post from the plate to the model, now
//!   carries more of the field's load: its tip ball 0.723 -> 0.915 mm and its
//!   area-law shaft 0.542 -> 0.686 mm radius, so its crowding disc (1.6x the
//!   root) laps its neighbour's by 0.19 mm. The drawn flares (1.006 and
//!   1.136 mm) do not touch (2.142 mm against 2.148 mm apart), and no cell
//!   stands under three roots. The run reports the pair on the congestion
//!   lane's line (`overlapping root pairs 16 -> 16 -> 5`). The other four
//!   pairs are the ones `for-craig-v15` shows, at the same centres: three on
//!   the post at (-2.987, 0.263) by the drain ring and one 12-13 mm out.
//!
//! The fixture is the PROVISIONED `corpus/models/Cylinder.stl` (gitignored, as
//! `round100_emit.rs` uses Round_100), run exactly as the CLI runs it
//! (`--orient dense --placement sweep --no-cache`).

use std::path::{Path, PathBuf};

use plinth_pipeline::{
    prepare, registered_decimation_budget, registered_support_request, ModelPreparationOutcome,
    OrientationPolicy, OutputProfile, PrepareRequest,
};

const STEM: &str = "Cylinder";
/// The raster the stack is counted on: the sweep's working pitch.
const RASTER_MM: f64 = 0.05;
/// See the module doc: the only overlapping root pairs allowed, each named by
/// its two root centres `(x, y)` in file coordinates (mm). Re-pinned at the
/// round-16 milestone from "at most four" (the reason is in the module doc).
const PINNED_PAIRS: [[(f64, f64); 2]; 5] = [
    [(-2.987, 0.263), (-2.587, -1.237)],
    [(-2.987, 0.263), (-2.187, 1.863)],
    [(-2.987, 0.263), (-4.737, 1.663)],
    [(-6.687, 0.763), (-4.737, 1.663)],
    [(5.763, -11.587), (5.463, -10.987)],
];
/// How near a root centre must be to a pinned centre to be that root. The
/// centres stand on the 0.05 mm working lattice, so a moved root misses by
/// 0.05 mm or more.
const CENTRE_MATCH_MM: f64 = 0.01;

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("plinth-drain-ring-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("in")).expect("scratch input folder");
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/plinth-pipeline sits two levels under the repo root")
        .to_path_buf()
}

/// The production request the CLI builds for
/// `plinth prepare --output saturn4-ultra --orient dense --placement sweep --no-cache`.
fn request(scratch: &Scratch) -> PrepareRequest {
    let printer = OutputProfile::Saturn4Ultra
        .resolve()
        .expect("M0 Saturn profile")
        .clone();
    PrepareRequest {
        input_models_dir: scratch.0.join("in"),
        output_dir: scratch.0.join("out"),
        orientation: OrientationPolicy::DenseSweep,
        no_orient: Default::default(),
        placement: plinth_pipeline::Placement::Sweep,
        output_profile: OutputProfile::Saturn4Ultra,
        decimation_budget: registered_decimation_budget().expect("registered budget"),
        hollow: plinth_hollow::HollowRequest::with_registered_wall_default(printer.clone())
            .expect("registered wall"),
        support: registered_support_request(&printer).expect("registered lift"),
        no_cache: true,
        exact_delicacy: false,
        stop_after: None,
    }
}

/// One plate root as the file draws it: centre and crowding-disc radius.
#[derive(Clone, Copy, Debug)]
struct Root {
    record: usize,
    x: f64,
    y: f64,
    disc: f64,
}

/// What a record is to the member grouping (see the module doc).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Part {
    Ball,
    Shaft,
    Flare,
    Brace,
    Other,
}

/// Every plate root in the file, read from its support records alone.
fn plate_roots(records: &[chitubox_io::SupportRecord], flare_factor: f64) -> Vec<Root> {
    use chitubox_io::SegmentType;
    let mut part = vec![Part::Other; records.len()];
    let mut owner = vec![usize::MAX; records.len()];
    let mut open = usize::MAX;
    for (index, record) in records.iter().enumerate() {
        let kind = record.segment_type;
        if kind == SegmentType::TIP {
            open = index;
            part[index] = Part::Ball;
            owner[index] = open;
        } else if kind == SegmentType::TAPER {
            let after_tip = index > 0 && records[index - 1].segment_type == SegmentType::TIP;
            let next = records.get(index + 1).map(|next| next.segment_type);
            if after_tip {
                part[index] = Part::Shaft;
                owner[index] = open;
            } else if next == Some(SegmentType::JOINER) || next == Some(SegmentType::FLARE) {
                open = index;
                part[index] = Part::Shaft;
                owner[index] = open;
            } else {
                part[index] = Part::Brace;
                open = usize::MAX;
            }
        } else if kind == SegmentType::JOINER && index > 0 {
            part[index] = Part::Shaft;
            owner[index] = owner[index - 1];
        } else if kind == SegmentType::FLARE && index > 0 {
            part[index] = Part::Flare;
            owner[index] = owner[index - 1];
        }
    }
    let mut roots = Vec::new();
    for (index, _) in records.iter().enumerate() {
        if part[index] != Part::Flare || owner[index] == usize::MAX {
            continue;
        }
        let member = owner[index];
        // The lowest shaft end of this member, and its radius there.
        let mut lowest: Option<(f64, f64, f64, f64)> = None;
        for (shaft, record) in records.iter().enumerate() {
            if owner[shaft] != member || part[shaft] != Part::Shaft {
                continue;
            }
            for (point, radius) in [(record.start, record.r_start), (record.end, record.r_end)] {
                let z = f64::from(point[2]);
                if lowest.is_none_or(|(low, _, _, _)| z < low) {
                    lowest = Some((z, f64::from(point[0]), f64::from(point[1]), f64::from(radius)));
                }
            }
        }
        if let Some((_, x, y, radius)) = lowest {
            roots.push(Root {
                record: member,
                x,
                y,
                disc: radius * flare_factor,
            });
        }
    }
    roots
}

fn overlapping_pairs(roots: &[Root]) -> Vec<(Root, Root, f64)> {
    let mut pairs = Vec::new();
    for (i, a) in roots.iter().enumerate() {
        for b in &roots[i + 1..] {
            let distance = (a.x - b.x).hypot(a.y - b.y);
            if distance < a.disc + b.disc {
                pairs.push((*a, *b, distance));
            }
        }
    }
    pairs
}

/// Whether a root stands at a pinned centre.
fn at(root: &Root, centre: (f64, f64)) -> bool {
    (root.x - centre.0).hypot(root.y - centre.1) <= CENTRE_MATCH_MM
}

/// Whether an overlapping pair is one of [`PINNED_PAIRS`], in either order.
fn pinned(a: &Root, b: &Root) -> bool {
    PINNED_PAIRS
        .iter()
        .any(|[p, q]| (at(a, *p) && at(b, *q)) || (at(a, *q) && at(b, *p)))
}

/// Raster cells (centres on a `RASTER_MM` lattice) under three or more discs.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "test-local lattice indices over a bounded plate"
)]
fn stacked_cells(roots: &[Root]) -> usize {
    let mut counts: std::collections::HashMap<(i64, i64), u8> = std::collections::HashMap::new();
    for root in roots {
        let lo_i = ((root.x - root.disc) / RASTER_MM).floor() as i64;
        let hi_i = ((root.x + root.disc) / RASTER_MM).ceil() as i64;
        let lo_j = ((root.y - root.disc) / RASTER_MM).floor() as i64;
        let hi_j = ((root.y + root.disc) / RASTER_MM).ceil() as i64;
        for i in lo_i..=hi_i {
            for j in lo_j..=hi_j {
                let (cx, cy) = ((i as f64 + 0.5) * RASTER_MM, (j as f64 + 0.5) * RASTER_MM);
                if (cx - root.x).hypot(cy - root.y) <= root.disc {
                    *counts.entry((i, j)).or_insert(0) += 1;
                }
            }
        }
    }
    counts.values().filter(|count| **count >= 3).count()
}

#[test]
fn the_cylinders_drain_ring_stands_no_pile_of_overlapping_posts() {
    let source = repo_root().join("corpus/models").join(format!("{STEM}.stl"));
    assert!(
        source.is_file(),
        "RequiredCorpusModelMissing: {} is not provisioned (corpus/models is gitignored)",
        source.display()
    );
    let scratch = Scratch::new();
    std::fs::copy(&source, scratch.0.join("in").join(format!("{STEM}.stl"))).expect("stage the Cylinder");
    let report = prepare(request(&scratch)).expect("the sweep route emits and warns");
    let entry = report
        .models
        .iter()
        .find(|entry| entry.model.as_str() == STEM)
        .expect("the Cylinder is in the report");
    let ModelPreparationOutcome::Artifact { emit, .. } = &entry.outcome else {
        panic!("the Cylinder must reach an artifact; it reported {:?}", entry.outcome);
    };
    let project = chitubox_io::Project::open(&emit.artifact_path).expect("the artifact must reopen");
    let records = project.supports(0).expect("support records");
    let flare_factor = plinth_core::m0_constants()
        .expect("registry")
        .placeholders()
        .flare_r_factor()
        .diagnostic()
        .get();
    let roots = plate_roots(&records, flare_factor);
    let pairs = overlapping_pairs(&roots);
    let stacked = stacked_cells(&roots);
    let named: Vec<_> = pairs
        .iter()
        .map(|(a, b, distance)| (a.record, (a.x, a.y), b.record, (b.x, b.y), *distance))
        .collect();
    println!(
        "{} plate roots; {} overlapping pairs (record, centre, record, centre, distance) {named:?}; \
         {stacked} cells under 3+ roots",
        roots.len(),
        pairs.len(),
    );
    assert!(roots.len() > 50, "the grader must find the Cylinder's plate roots, found {}", roots.len());
    assert_eq!(stacked, 0, "no cell may stand under three or more plate roots");
    let unpinned: Vec<_> = pairs.iter().filter(|(a, b, _)| !pinned(a, b)).collect();
    assert!(
        unpinned.is_empty(),
        "no overlapping root pair outside the pinned {} (16 pairs on for-craig-v11); new: {unpinned:?}",
        PINNED_PAIRS.len()
    );
}
