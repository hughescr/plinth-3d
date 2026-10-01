//! **An orientation score must not depend on how the mesh is triangulated.**
//!
//! Orchestrator ruling 2026-10-01 (scar-fold-invariance lane). The
//! toed-slab-fixture lane (`plinth-evidence/slice-sweep-2026-09-02/
//! field-2026-09-28/lanes/toed-slab-fixture.md`) found the fragility fold
//! charging each face `0.6 x` the mean angle defect of its three corners over
//! the face's WHOLE area: a flat slab top made of two triangles whose corners
//! are all cube corners paid 135.000 raw mm², the same flat area made of 70
//! triangles paid 81.059, and that difference decided the pose.
//!
//! The witness here is one box, the same solid, at three tessellations:
//! two triangles per face (only the 8 cube corners as vertices), a uniform
//! 8 x 8 grid per face, and an irregular grid with cells from 1 mm to 7 mm.
//! Every vertex coordinate is an integer, so the three meshes describe the
//! same surface exactly and weld exactly. Every term the analytic scorer
//! publishes must agree across the three to a small relative tolerance, at
//! every candidate pose.

use std::collections::HashMap;

use plinth_mesh::Mesh;
use plinth_orient::{
    score_candidates, CandidateOutcome, CandidateTerms, ObjectiveWeights, OrientFaceTable,
    OrientSearchLimits, UpDirection,
};

/// The box spans `[-16, 16] x [-8, 8] x [0, 8]` mm. Each axis is cut at the
/// given integer breakpoints (both ends included), and every face of the box
/// is gridded by the breakpoints of its two in-plane axes.
struct Cuts {
    x: Vec<f64>,
    y: Vec<f64>,
    z: Vec<f64>,
}

fn coarse() -> Cuts {
    Cuts {
        x: vec![-16.0, 16.0],
        y: vec![-8.0, 8.0],
        z: vec![0.0, 8.0],
    }
}

fn uniform() -> Cuts {
    let steps = |lo: f64, hi: f64| -> Vec<f64> {
        (0..=8)
            .map(|i| lo + (hi - lo) * f64::from(i) / 8.0)
            .collect()
    };
    Cuts {
        x: steps(-16.0, 16.0),
        y: steps(-8.0, 8.0),
        z: steps(0.0, 8.0),
    }
}

fn irregular() -> Cuts {
    Cuts {
        x: vec![-16.0, -15.0, -11.0, -4.0, 0.0, 7.0, 13.0, 16.0],
        y: vec![-8.0, -7.0, -2.0, 3.0, 6.0, 8.0],
        z: vec![0.0, 1.0, 3.0, 7.0, 8.0],
    }
}

/// A closed, outward-wound box gridded by `cuts`. Each face is listed as the
/// fixed axis, which end it sits at, and the in-plane `(u, v)` axes ordered so
/// that `u x v` is the outward normal; each grid cell becomes two triangles.
fn gridded_box(cuts: &Cuts) -> Mesh {
    let axes = [&cuts.x, &cuts.y, &cuts.z];
    // (fixed axis, at max end?, u axis, v axis) with u x v = outward normal.
    let faces = [
        (0, true, 1, 2),
        (0, false, 2, 1),
        (1, true, 2, 0),
        (1, false, 0, 2),
        (2, true, 0, 1),
        (2, false, 1, 0),
    ];
    let mut index: HashMap<[usize; 3], u32> = HashMap::new();
    let mut verts: Vec<[f64; 3]> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut vertex = |grid: [usize; 3], verts: &mut Vec<[f64; 3]>| -> u32 {
        *index.entry(grid).or_insert_with(|| {
            verts.push([cuts.x[grid[0]], cuts.y[grid[1]], cuts.z[grid[2]]]);
            u32::try_from(verts.len() - 1).expect("small mesh")
        })
    };
    for (fixed, at_max, u, v) in faces {
        let fixed_index = if at_max { axes[fixed].len() - 1 } else { 0 };
        for a in 0..axes[u].len() - 1 {
            for b in 0..axes[v].len() - 1 {
                let corner = |da: usize, db: usize| {
                    let mut grid = [0_usize; 3];
                    grid[fixed] = fixed_index;
                    grid[u] = a + da;
                    grid[v] = b + db;
                    grid
                };
                let p00 = vertex(corner(0, 0), &mut verts);
                let p10 = vertex(corner(1, 0), &mut verts);
                let p11 = vertex(corner(1, 1), &mut verts);
                let p01 = vertex(corner(0, 1), &mut verts);
                tris.push([p00, p10, p11]);
                tris.push([p00, p11, p01]);
            }
        }
    }
    Mesh::new(verts, tris).expect("a gridded box is a valid mesh")
}

fn candidates() -> Vec<UpDirection> {
    let s = std::f64::consts::FRAC_1_SQRT_2;
    let t = 1.0 / 3.0_f64.sqrt();
    [
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [s, 0.0, s],
        [0.0, -s, -s],
        [t, t, t],
        [-t, t, -t],
    ]
    .into_iter()
    .map(|direction| UpDirection::new(direction).expect("unit direction"))
    .collect()
}

fn scored(mesh: &Mesh) -> Vec<CandidateTerms> {
    let table = OrientFaceTable::build(mesh, "tessellation-invariance").expect("face table");
    let weights = ObjectiveWeights::registered().expect("registered weights");
    let outcome = score_candidates(
        &table,
        &candidates(),
        weights,
        OrientSearchLimits::production(),
    )
    .expect("the box is scorable");
    outcome
        .outcomes()
        .iter()
        .map(|outcome| match outcome {
            CandidateOutcome::Scored(terms) => *terms,
            CandidateOutcome::Refused(refusal) => panic!("a box pose was refused: {refusal:?}"),
        })
        .collect()
}

fn named(terms: &CandidateTerms) -> [(&'static str, f64); 9] {
    [
        ("support_area_cm2", terms.support_area_cm2),
        ("peak_xsec_cm2", terms.peak_xsec_cm2),
        ("peel_term", terms.peel_term),
        ("height_mm", terms.height_mm),
        ("scar_term", terms.scar_term),
        ("scar_raw_mm2", terms.scar_raw_mm2),
        ("fragility_raw_mm2", terms.fragility_raw_mm2),
        ("display_raw_mm2", terms.display_raw_mm2),
        ("score", terms.score),
    ]
}

/// Relative agreement, with an absolute allowance at the scale of the term's
/// own float noise for terms that are zero by construction (display on a
/// box with no detail).
fn agree(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * a.abs().max(b.abs()) + 1e-12
}

#[test]
fn the_same_box_scores_the_same_at_three_tessellations() {
    let tessellations = [
        ("2 triangles per face", gridded_box(&coarse())),
        ("uniform 8 x 8 grid", gridded_box(&uniform())),
        ("irregular grid", gridded_box(&irregular())),
    ];
    // Path-hit witness: three genuinely different meshes of one box.
    let face_counts: Vec<usize> = tessellations
        .iter()
        .map(|(_, mesh)| mesh.tris().len())
        .collect();
    assert_eq!(face_counts[0], 12, "the coarse box is two triangles per face");
    assert!(
        face_counts[1] > 12 && face_counts[2] > 12 && face_counts[1] != face_counts[2],
        "the fine tessellations must differ from the coarse one and from each other: {face_counts:?}"
    );

    let results: Vec<Vec<CandidateTerms>> = tessellations
        .iter()
        .map(|(_, mesh)| scored(mesh))
        .collect();

    let mut mismatches = Vec::new();
    for (pose, reference) in results[0].iter().enumerate() {
        for (which, other) in results.iter().enumerate().skip(1) {
            for ((name, a), (_, b)) in named(reference).iter().zip(named(&other[pose]).iter()) {
                println!(
                    "pose {pose} {name}: {} {a:.9} | {} {b:.9}",
                    tessellations[0].0, tessellations[which].0
                );
                if !agree(*a, *b) {
                    mismatches.push(format!(
                        "pose {pose} {name}: {} = {a:.9}, {} = {b:.9}",
                        tessellations[0].0, tessellations[which].0
                    ));
                }
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "the score depends on the triangulation:\n{}",
        mismatches.join("\n")
    );
    // The fold is actually measured, not vacuously zero: every pose that turns
    // a box face down charges that face's corner curvature. (The two
    // body-diagonal poses turn no face past the overhang limit, so their
    // support and fold are zero by construction; they still grade the peak
    // section and the height.)
    let mut poses_with_a_fold = 0_usize;
    for (pose, terms) in results[0].iter().enumerate() {
        if terms.support_area_cm2 > 0.0 {
            assert!(
                terms.fragility_raw_mm2 > 0.0,
                "pose {pose} turns {} cm2 down but folds no fragility; the witness measured \
                 nothing",
                terms.support_area_cm2
            );
            poses_with_a_fold += 1;
        }
    }
    assert!(
        poses_with_a_fold >= 8,
        "only {poses_with_a_fold} poses turned a face down"
    );
}
