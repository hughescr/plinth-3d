//! Print every objective term for a fixture in its two flip poses, so a pose
//! decision can be read off term by term instead of inferred from the winner.
//!
//! `cargo run -p plinth-orient --release --example pose_terms_probe -- [fixture]`
//! where fixture is one of: raking_blade (default), toed_slab, overhang_table.

use plinth_orient::{
    score_candidates, CandidateOutcome, ObjectiveWeights, OrientFaceTable, OrientSearchLimits,
    UpDirection,
};

fn main() {
    let which = std::env::args().nth(1).unwrap_or_else(|| "raking_blade".to_string());
    let mesh = match which.as_str() {
        "toed_slab" => plinth_mesh::make_toed_slab(),
        "overhang_table" => plinth_mesh::make_overhang_table(),
        _ => plinth_mesh::make_raking_blade(),
    }
    .expect("fixture");
    let table = OrientFaceTable::build(&mesh, "probe").expect("face table");
    let weights = ObjectiveWeights::registered().expect("weights");
    let poses = [
        ("+Z (identity)", [0.0, 0.0, 1.0]),
        ("-Z (inverted)", [0.0, 0.0, -1.0]),
    ];
    let candidates: Vec<UpDirection> = poses
        .iter()
        .map(|(_, d)| UpDirection::new(*d).expect("unit"))
        .collect();
    let scored = score_candidates(&table, &candidates, weights, OrientSearchLimits::production())
        .expect("scored");
    println!("fixture {which}: {} faces", mesh.tris().len());
    for ((name, _), outcome) in poses.iter().zip(scored.outcomes()) {
        match outcome {
            CandidateOutcome::Scored(t) => println!(
                "{name}: score {:.5} | support_area {:.5} cm2 | height {:.3} mm | peel_term {:.5} | \
                 scar_term {:.5} = raw {:.4} mm2 (fragility {:.4} + display {:.4}) over {} down faces",
                t.score,
                t.support_area_cm2,
                t.height_mm,
                t.peel_term,
                t.scar_term,
                t.scar_raw_mm2,
                t.fragility_raw_mm2,
                t.display_raw_mm2,
                t.down_facing_faces
            ),
            CandidateOutcome::Refused(r) => println!("{name}: refused {r:?}"),
        }
    }
}
