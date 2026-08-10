//! The orientation scoring substrate, checked against geometry nobody can
//! argue with.
//!
//! Three of the recorded §7 tests live here: **T3** the analytic sphere, which
//! pins the units and the overhang cutoff without reference to any committed
//! artifact; **T6** the z-spin null direction, which machine-checks the claim
//! that makes the search space S² instead of SO(3); and **T14** the section
//! oracle, the single test that licenses skipping polygon stitching in the hot
//! loop.

use geom_kernel::{NativeSurface, NativeSurfaceConfig, PlaneSliceLimits};
use plinth_mesh::{
    make_box, make_box_default, make_overhang_table, make_thin_fin, make_toed_slab, Mesh,
};
use plinth_orient::{
    score_candidates, section_area_mm2_at, CandidateOutcome, CandidateTerms, ObjectiveWeights,
    OrientFaceTable, OrientSearchLimits, UpDirection,
};

fn table(mesh: &Mesh) -> OrientFaceTable {
    OrientFaceTable::build(mesh, "test-lod").expect("scoring table for a closed fixture")
}

fn score_all(
    table: &OrientFaceTable,
    candidates: &[UpDirection],
    weights: ObjectiveWeights,
) -> Vec<CandidateTerms> {
    let limits = OrientSearchLimits::production(table.face_count(), table.vertex_count());
    let scored =
        score_candidates(table, candidates, weights, limits).expect("scoring must not refuse");
    scored
        .outcomes()
        .iter()
        .map(|outcome| match outcome {
            CandidateOutcome::Scored(terms) => *terms,
            CandidateOutcome::Refused(refusal) => panic!("candidate refused: {refusal:?}"),
        })
        .collect()
}

fn assert_close(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: expected {expected}, got {actual} (tolerance {tolerance})"
    );
}

/// A 10 mm cube has no term this scorer can round away, so every one of them is
/// asserted exactly: one down-facing face of 1 cm², a 1 cm² section at every
/// height, 10 mm tall, and — since a cube corner's welded angle defect is
/// exactly 0.25 and a 10 mm wall is far thicker than `THIN_FEATURE_MM` — a
/// delicacy of exactly `0.6·0.25 + 0.4·0 = 0.15` on every face.
#[test]
fn a_cube_measures_every_term_analytically_including_the_scar() {
    let mesh = make_box_default().expect("cube fixture");
    let table = table(&mesh);
    let weights = ObjectiveWeights::registered().expect("registered objective");
    let terms = score_all(&table, &[UpDirection::MODEL_UP], weights);
    let terms = terms[0];

    assert_close(terms.support_area_cm2, 1.0, 1e-12, "support area");
    assert_close(terms.peak_xsec_cm2, 1.0, 1e-12, "peak cross-section");
    assert_close(terms.height_mm, 10.0, 1e-12, "height");
    assert_eq!(
        terms.down_facing_faces, 2,
        "the cube's floor is two triangles"
    );
    // 100 mm² of floor at delicacy 0.15.
    assert_close(terms.scar_raw_mm2, 15.0, 1e-12, "raw scar");
    assert_close(terms.scar_term, 0.4 * 15.0 / 100.0, 1e-12, "scar term");
    // 0.5·1 + 3·1 + 0.3·10 + 0.06.
    assert_close(terms.score, 6.56, 1e-12, "score");

    // Turning the cube on its side is the same cube: every term is unchanged.
    let sideways = score_all(
        &table,
        &[UpDirection::new([1.0, 0.0, 0.0]).expect("unit x")],
        weights,
    )[0];
    assert_close(sideways.score, terms.score, 1e-12, "score about x");
}

/// **T3.** A 20 mm sphere pins the units and the cutoff analytically: height is
/// 20 mm, the peak section is `πr²`, and the down-facing cap is the zone below
/// `n_z < −sin 45°`, i.e. `2πr²(1 + cos 135°)`.  The scar-free total is 16.345,
/// which is also the analytic cross-check the corpus fixture's recorded 16.6 is
/// read against.
///
/// The tolerances are the tessellation's, not the solver's: an inscribed
/// polyhedron's equatorial section is short of `πr²` by `O(Δθ²)` and the cap's
/// boundary quantizes to whole rings.
#[test]
fn an_analytic_sphere_pins_the_units_and_the_overhang_cutoff() {
    let radius = 10.0;
    let mesh = uv_sphere(radius, 180, 360);
    let table = table(&mesh);
    let weights = ObjectiveWeights::registered()
        .expect("registered objective")
        .scar_free();
    let terms = score_all(&table, &[UpDirection::MODEL_UP], weights)[0];

    let peak = std::f64::consts::PI * radius * radius / 100.0;
    let cap = 2.0 * std::f64::consts::PI * radius * radius * (1.0 + (135.0_f64).to_radians().cos())
        / 100.0;
    // The poles are exact vertices, so the height is not an approximation.
    assert_close(terms.height_mm, 2.0 * radius, 1e-12, "height");
    assert_close(terms.peak_xsec_cm2, peak, peak * 1e-3, "peak cross-section");
    assert_close(terms.support_area_cm2, cap, cap * 2e-2, "support cap");
    assert_close(terms.scar_term, 0.0, 0.0, "scar is off");
    assert_close(terms.score, 16.34494, 0.05, "scar-free total");
}

/// **T6.** Rotating the model about the candidate direction is a null operation
/// on every term: the search space is S², and a k-fold z-spin sweep would cost
/// k× for exactly nothing.  The down-facing face count is asserted **exactly**,
/// not approximately — a spin cannot move a face across the overhang cutoff.
#[test]
fn z_spin_is_a_null_direction_of_every_term() {
    let weights = ObjectiveWeights::registered().expect("registered objective");
    for mesh in [
        make_box([12.0, 7.0, 20.0], [3.0, -2.0, 5.0]).expect("box fixture"),
        make_thin_fin().expect("thin fin fixture"),
        make_overhang_table().expect("overhang table fixture"),
    ] {
        for direction in [[0.0, 0.0, 1.0], [0.3, -0.7, 0.5], [-0.2, 0.1, -0.9]] {
            let up = UpDirection::new(direction).expect("unit direction");
            let original = table(&mesh);
            let baseline = score_all(&original, &[up], weights)[0];

            for spin_degrees in [37.0_f64, 180.0, -113.0] {
                let spun = rotate_about(&mesh, up.get(), spin_degrees.to_radians());
                let spun_table = table(&spun);
                let spun_terms = score_all(&spun_table, &[up], weights)[0];
                assert_eq!(
                    spun_terms.down_facing_faces, baseline.down_facing_faces,
                    "a spin moved a face across the overhang cutoff"
                );
                for (actual, expected, what) in [
                    (
                        spun_terms.support_area_cm2,
                        baseline.support_area_cm2,
                        "support area",
                    ),
                    (
                        spun_terms.peak_xsec_cm2,
                        baseline.peak_xsec_cm2,
                        "peak cross-section",
                    ),
                    (spun_terms.height_mm, baseline.height_mm, "height"),
                    (spun_terms.scar_term, baseline.scar_term, "scar term"),
                    (spun_terms.score, baseline.score, "score"),
                ] {
                    assert_close(
                        actual,
                        expected,
                        1e-9 * expected.abs().max(1.0),
                        &format!("{what} under a {spin_degrees}° spin"),
                    );
                }
            }
        }
    }
}

/// **T14.** The piecewise-quadratic divergence area agrees with the geometry
/// kernel's own plane-slice batch, which shares none of this solver's machinery:
/// the kernel finds crossings by BVH descent and Möller intersection, and the
/// oracle below orients each chord by the `ẑ × n` dot-product test that the
/// solver deliberately does **not** use.  This is the test that licenses
/// skipping loop stitching and polygon union in the inner loop.
#[test]
fn the_section_solver_agrees_with_the_kernel_slice_oracle() {
    for mesh in [
        make_box_default().expect("cube fixture"),
        make_thin_fin().expect("thin fin fixture"),
        make_overhang_table().expect("overhang table fixture"),
        make_toed_slab().expect("toed slab fixture"),
    ] {
        let table = table(&mesh);
        for direction in [[0.0, 0.0, 1.0], [0.2, 0.5, 0.84], [-0.6, 0.0, -0.8]] {
            let up = UpDirection::new(direction).expect("unit direction");
            let rotation = up.rotation_to_build_frame();
            let build_frame = apply_rotation(&mesh, rotation);
            let heights = sample_heights(&build_frame);
            let oracle = kernel_section_areas(&build_frame, &heights);

            for (height, expected) in heights.iter().zip(&oracle) {
                let actual = section_area_mm2_at(&table, up, *height)
                    .expect("the section area must be finite");
                assert_close(
                    actual,
                    *expected,
                    1e-9 * expected.abs().max(1.0),
                    &format!("section at {height} mm along {direction:?}"),
                );
            }

            // The supremum is a supremum: no sampled section may exceed it.
            let peak = score_all(
                &table,
                &[up],
                ObjectiveWeights::registered().expect("registered objective"),
            )[0]
            .peak_xsec_cm2
                * 100.0;
            for area in &oracle {
                assert!(
                    *area <= peak + 1e-9 * peak.abs().max(1.0),
                    "a sampled section {area} exceeded the reported peak {peak}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Fixtures and the independent oracle.
// ---------------------------------------------------------------------------

/// A closed, outward-wound UV sphere with exact poles.
fn uv_sphere(radius: f64, latitudes: usize, longitudes: usize) -> Mesh {
    let mut verts = Vec::with_capacity(2 + (latitudes - 1) * longitudes);
    verts.push([0.0, 0.0, radius]);
    for ring in 1..latitudes {
        let phi = std::f64::consts::PI * ring as f64 / latitudes as f64;
        let (sin_phi, cos_phi) = phi.sin_cos();
        for step in 0..longitudes {
            let theta = 2.0 * std::f64::consts::PI * step as f64 / longitudes as f64;
            let (sin_theta, cos_theta) = theta.sin_cos();
            verts.push([
                radius * sin_phi * cos_theta,
                radius * sin_phi * sin_theta,
                radius * cos_phi,
            ]);
        }
    }
    let south = verts.len() as u32;
    verts.push([0.0, 0.0, -radius]);

    let ring_vertex = |ring: usize, step: usize| -> u32 {
        (1 + (ring - 1) * longitudes + step % longitudes) as u32
    };
    let mut tris = Vec::new();
    for step in 0..longitudes {
        tris.push([0, ring_vertex(1, step), ring_vertex(1, step + 1)]);
    }
    for ring in 1..latitudes - 1 {
        for step in 0..longitudes {
            let upper = [ring_vertex(ring, step), ring_vertex(ring, step + 1)];
            let lower = [ring_vertex(ring + 1, step), ring_vertex(ring + 1, step + 1)];
            tris.push([upper[0], lower[0], lower[1]]);
            tris.push([upper[0], lower[1], upper[1]]);
        }
    }
    for step in 0..longitudes {
        tris.push([
            south,
            ring_vertex(latitudes - 1, step + 1),
            ring_vertex(latitudes - 1, step),
        ]);
    }
    Mesh::new(verts, tris).expect("uv sphere is a valid indexed mesh")
}

fn apply_rotation(mesh: &Mesh, rotation: [[f64; 3]; 3]) -> Mesh {
    let verts = mesh
        .verts()
        .iter()
        .map(|vertex| {
            [
                rotation[0][0] * vertex[0]
                    + rotation[0][1] * vertex[1]
                    + rotation[0][2] * vertex[2],
                rotation[1][0] * vertex[0]
                    + rotation[1][1] * vertex[1]
                    + rotation[1][2] * vertex[2],
                rotation[2][0] * vertex[0]
                    + rotation[2][1] * vertex[1]
                    + rotation[2][2] * vertex[2],
            ]
        })
        .collect();
    Mesh::new(verts, mesh.tris().to_vec()).expect("a rotated mesh is still a valid indexed mesh")
}

/// Rodrigues rotation of the whole model about an arbitrary axis.
fn rotate_about(mesh: &Mesh, axis: [f64; 3], radians: f64) -> Mesh {
    let (sin, cos) = radians.sin_cos();
    let [x, y, z] = axis;
    let one = 1.0 - cos;
    let rotation = [
        [
            cos + x * x * one,
            x * y * one - z * sin,
            x * z * one + y * sin,
        ],
        [
            y * x * one + z * sin,
            cos + y * y * one,
            y * z * one - x * sin,
        ],
        [
            z * x * one - y * sin,
            z * y * one + x * sin,
            cos + z * z * one,
        ],
    ];
    apply_rotation(mesh, rotation)
}

/// Interior heights that avoid every vertex height, so the oracle sees only
/// ordinary edge crossings and the comparison is about area rather than about
/// degenerate-touch bookkeeping.
fn sample_heights(mesh: &Mesh) -> Vec<f64> {
    let mut heights: Vec<f64> = mesh.verts().iter().map(|vertex| vertex[2]).collect();
    heights.sort_by(f64::total_cmp);
    heights.dedup();
    let mut samples = Vec::new();
    for window in heights.windows(2) {
        let (low, high) = (window[0], window[1]);
        if high - low > 1e-6 {
            samples.push(0.5 * (low + high));
        }
    }
    // A handful spread across the model is enough; the point is agreement, not
    // coverage of every interval.
    let stride = (samples.len() / 8).max(1);
    samples.into_iter().step_by(stride).collect()
}

/// The independent oracle: the kernel's own plane-slice batch, with each chord
/// oriented by the `ẑ × n` test and summed by shoelace.
fn kernel_section_areas(mesh: &Mesh, heights: &[f64]) -> Vec<f64> {
    let estimate =
        NativeSurface::estimate_index_memory_from_indexed(mesh.verts(), mesh.tris(), 1.0)
            .expect("kernel index estimate");
    let surface = NativeSurface::from_indexed(
        mesh.verts(),
        mesh.tris(),
        NativeSurfaceConfig {
            grid_pitch_mm: 1.0,
            max_grid_bytes: estimate.max_axis_grid_bytes(),
        },
    )
    .expect("kernel surface");
    let batch = surface
        .slice_z_batch(
            heights,
            PlaneSliceLimits {
                max_planes: heights.len().max(1),
                max_triangle_plane_tests: usize::MAX,
                max_intersections: usize::MAX,
                max_output_bytes: usize::MAX,
            },
        )
        .expect("kernel plane slice");
    let normals = surface.geometry().normals();

    (0..heights.len())
        .map(|plane| {
            let mut area = 0.0;
            for intersection in batch.plane(plane) {
                let normal = normals[intersection.face_id as usize];
                // Interior on the left means the chord runs along ẑ × n.
                let along = [-normal[1], normal[0]];
                let [start, end] = intersection.segment;
                let delta = [end[0] - start[0], end[1] - start[1]];
                let (p, q) = if delta[0] * along[0] + delta[1] * along[1] >= 0.0 {
                    (start, end)
                } else {
                    (end, start)
                };
                area += 0.5 * (p[0] * q[1] - p[1] * q[0]);
            }
            area
        })
        .collect()
}
