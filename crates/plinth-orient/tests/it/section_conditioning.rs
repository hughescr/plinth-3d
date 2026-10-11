//! **The peak section does not depend on whether a flat facet lies exactly on
//! a breakpoint.**
//!
//! Lane 0b (`plinth-evidence/.../wave1b/STAGE-JITTER.md`, 2026-10-11) measured
//! the 62 congruent side poses of the corpus Cylinder at 75 to 1024 mm², and
//! one at 6,584,598 mm², when every one of them is the same 30 x 30 mm section.
//! Each side pose lays one facet horizontal. In floating point that facet's
//! corners land a few ulps apart in height, so the facet becomes a sliver of
//! the height axis (1.8e-15 mm on the reference winner). A face's quadratic,
//! written in the absolute height `t`, has coefficients of order `(L/w)^2` for
//! a sliver of height `w`: 7e30 there. The solver used to sum every face's
//! coefficients into one prefix-summed running polynomial, so one sliver
//! erased every other face's coefficients for the rest of the height range,
//! and the section function went negative by megasquare millimetres.
//!
//! These tests pin the physical statement instead of the arithmetic one: a
//! facet tilted by a few ulps, or lifted or dropped by a rounding-sized step
//! against the other vertices at its height, changes the peak section by no
//! more than the geometry itself changes.

use geom_kernel::vec3;
use plinth_mesh::{make_box, make_disc_fan, make_overhang_table, Mesh};
use plinth_orient::{
    score_candidates, CandidateOutcome, ObjectiveWeights, OrientFaceTable, OrientSearchLimits,
    UpDirection,
};

/// Peak section in mm² for each direction, by the production scoring path.
fn peaks(mesh: &Mesh, directions: &[[f64; 3]]) -> Vec<f64> {
    let table = OrientFaceTable::build(mesh, "test-lod").expect("closed fixture");
    let candidates: Vec<UpDirection> = directions
        .iter()
        .map(|direction| UpDirection::new(*direction).expect("finite direction"))
        .collect();
    let scored = score_candidates(
        &table,
        &candidates,
        ObjectiveWeights::registered().expect("registered objective"),
        OrientSearchLimits::production(),
    )
    .expect("scoring must not refuse");
    scored
        .outcomes()
        .iter()
        .map(|outcome| match outcome {
            CandidateOutcome::Scored(terms) => terms.peak_mm2,
            CandidateOutcome::Refused(refusal) => panic!("candidate refused: {refusal:?}"),
        })
        .collect()
}

/// `mesh` with every coordinate stored as f32, the way an STL carries it.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the narrowing to f32 is the point: it models what an STL stores"
)]
fn as_stored(mesh: &Mesh) -> Mesh {
    let verts = mesh
        .verts()
        .iter()
        .map(|vertex| vertex.map(|coordinate| f64::from(coordinate as f32)))
        .collect();
    Mesh::new(verts, mesh.tris().to_vec()).expect("stored fixture")
}

/// `mesh` rotated about +Z by `radians` and scaled by `1 + ulps * 2^-23`, then
/// stored as f32: `plinth-audit stl-jitter`, the input jitter Lane 0b used.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the narrowing to f32 is the point: it models what an STL stores"
)]
fn jittered(mesh: &Mesh, radians: f64, ulps: f64) -> Mesh {
    let (sin, cos) = radians.sin_cos();
    let scale = 1.0 + ulps * f64::powi(2.0, -23);
    let verts = mesh
        .verts()
        .iter()
        .map(|[x, y, z]| {
            [
                f64::from(((cos * x - sin * y) * scale) as f32),
                f64::from(((sin * x + cos * y) * scale) as f32),
                f64::from((z * scale) as f32),
            ]
        })
        .collect();
    Mesh::new(verts, mesh.tris().to_vec()).expect("jittered fixture")
}

/// `mesh` with each vertex at exactly height `at` that `only` selects moved up
/// by `step`.
fn with_level_moved(
    mesh: &Mesh,
    at: f64,
    step: f64,
    mut only: impl FnMut(&[f64; 3]) -> bool,
) -> Mesh {
    #[allow(
        clippy::float_cmp,
        reason = "the fixture's level is an exact authored value"
    )]
    let verts = mesh
        .verts()
        .iter()
        .map(|vertex| {
            let mut moved = *vertex;
            if vertex[2] == at && only(vertex) {
                moved[2] += step;
            }
            moved
        })
        .collect();
    Mesh::new(verts, mesh.tris().to_vec()).expect("perturbed fixture")
}

/// One closed shell: the L profile `(0,0) (10,0) (10,10) (20,10) (20,20)
/// (0,20)` in `(x, z)`, extruded 30 mm along +y. Its edge `(10,10)-(20,10)` is
/// a down-facing ledge at z = 10, a height the inner corner `(10,10)` also
/// holds, so the ledge lies exactly on a breakpoint. Section: 300 mm² below
/// the ledge, 600 mm² above it.
fn ledge_prism() -> Mesh {
    let profile = [
        [0.0, 0.0],
        [10.0, 0.0],
        [10.0, 10.0],
        [20.0, 10.0],
        [20.0, 20.0],
        [0.0, 20.0],
    ];
    let length = 30.0;
    let mut verts = Vec::new();
    for y in [0.0, length] {
        for [x, z] in profile {
            verts.push([x, y, z]);
        }
    }
    // The profile is counter-clockwise in (x, z), so a triangle listed in
    // profile order faces -y; the far cap takes the reverse order.
    let cap = [[0_u32, 1, 2], [0, 2, 5], [2, 3, 4], [2, 4, 5]];
    let mut tris = Vec::new();
    for [a, b, c] in cap {
        tris.push([a, b, c]);
        tris.push([a + 6, c + 6, b + 6]);
    }
    for i in 0..6_u32 {
        let j = (i + 1) % 6;
        tris.push([i, j + 6, j]);
        tris.push([i, i + 6, j + 6]);
    }
    Mesh::new(verts, tris).expect("ledge prism")
}

/// A unit direction tilted from +Z by `angle` radians toward `azimuth`.
fn tilted(angle: f64, azimuth: f64) -> [f64; 3] {
    let (sin, cos) = angle.sin_cos();
    let (sin_azimuth, cos_azimuth) = azimuth.sin_cos();
    [sin * cos_azimuth, sin * sin_azimuth, cos]
}

fn assert_relative(actual: f64, expected: f64, relative: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= relative * expected.abs(),
        "{what}: expected {expected} mm², got {actual} mm² (relative tolerance {relative:e})"
    );
}

/// Tilts that put a horizontal facet's corners a few ulps to a few hundred
/// nanometres apart in height: the rotation rounding of a rest pose, and the
/// f32 storage noise of an STL.
const TILTS: [f64; 6] = [1e-16, 1e-15, 1e-13, 1e-11, 1e-9, 1e-7];

/// One bound for every tilt and step below: at `<= 1e-7` rad or `<= 1e-6` mm
/// the true section of each fixture moves by less than 1e-6 of itself
/// (a 30 mm facet tilted 1e-7 rad changes its section by ~1e-14 relative; a
/// 1e-6 mm step on a 30 mm facet sweeps at most 1e-6 x 30 x 30 / 2 of slab
/// volume, i.e. the area of a sliver one step tall, far under one 0.05 mm layer).
const RELATIVE: f64 = 1e-6;

/// A box's floor and roof are flat facets at the two ends of the height range.
/// Tilting the build direction by a few ulps must not move the peak.
#[test]
fn a_box_tilted_by_rounding_keeps_its_peak() {
    // At the origin, and where a part sits on a real build plate.
    for translate in [[0.0, 0.0, 0.0], [118.5, -71.25, 3.0]] {
        let mesh = make_box([30.0, 30.0, 30.0], translate).expect("box");
        let mut directions = vec![[0.0, 0.0, 1.0], [0.0, 0.0, -1.0]];
        for tilt in TILTS {
            for azimuth in [0.0, 0.7, 2.1, 3.9] {
                directions.push(tilted(tilt, azimuth));
                directions.push(tilted(tilt, azimuth).map(|c| -c));
            }
        }
        let measured = peaks(&mesh, &directions);
        for (direction, peak) in directions.iter().zip(&measured) {
            assert_relative(
                *peak,
                900.0,
                RELATIVE,
                &format!("30 mm box at {translate:?} along {direction:?}"),
            );
        }
    }
}

/// A down-facing ledge lies exactly on a breakpoint (the inner corner holds the
/// same height). Moving the ledge's outer edge just above or just below that
/// breakpoint, one end or both, or tilting the whole part by a few ulps, must
/// leave the peak at the section above the ledge.
#[test]
#[allow(
    clippy::float_cmp,
    reason = "vertices are selected by their exact authored coordinates"
)]
fn a_facet_at_just_above_or_just_below_a_breakpoint_keeps_its_peak() {
    let ledge = ledge_prism();
    let reference = peaks(&ledge, &[[0.0, 0.0, 1.0]])[0];
    assert_relative(reference, 600.0, 1e-12, "the section above the ledge");

    let mut directions = vec![[0.0, 0.0, 1.0]];
    for tilt in TILTS {
        for azimuth in [0.0, 0.7, 2.1, 3.9] {
            directions.push(tilted(tilt, azimuth));
        }
    }
    for (direction, peak) in directions.iter().zip(peaks(&ledge, &directions)) {
        assert_relative(
            peak,
            reference,
            RELATIVE,
            &format!("ledge along {direction:?}"),
        );
    }

    // The outer edge of the ledge is the two vertices at (20, y, 10).
    for step in [1e-15, 1e-12, 1e-9, 1e-6] {
        for signed in [step, -step] {
            let both = with_level_moved(&ledge, 10.0, signed, |vertex| vertex[0] == 20.0);
            let near_end = with_level_moved(&ledge, 10.0, signed, |vertex| {
                vertex[0] == 20.0 && vertex[1] == 0.0
            });
            for (what, moved) in [("outer edge", both), ("one outer corner", near_end)] {
                let peak = peaks(&moved, &[[0.0, 0.0, 1.0]])[0];
                assert_relative(
                    peak,
                    reference,
                    RELATIVE,
                    &format!("ledge {what} moved {signed:e} mm"),
                );
            }
        }
    }

    // The overhang table's slab underside sits at the leg tops' height; tilting
    // the part by a few ulps spreads both across a sliver of the height axis.
    let table = make_overhang_table().expect("overhang table");
    let slab = peaks(&table, &[[0.0, 0.0, 1.0]])[0];
    assert_relative(slab, 900.0, 1e-12, "the table's slab section");
    for (direction, peak) in directions.iter().zip(peaks(&table, &directions)) {
        assert_relative(peak, slab, RELATIVE, &format!("table along {direction:?}"));
    }

    // One corner of a box's roof lifted or dropped: the roof's two triangles
    // are no longer coplanar, one corner just off the other three's level.
    let cube = make_box([30.0, 30.0, 30.0], [0.0, 0.0, 0.0]).expect("box");
    for step in [1e-15, 1e-12, 1e-9, 1e-6] {
        for signed in [step, -step] {
            let mut first = true;
            let moved = with_level_moved(&cube, 30.0, signed, |_| std::mem::take(&mut first));
            let peak = peaks(&moved, &[[0.0, 0.0, 1.0]])[0];
            assert_relative(
                peak,
                900.0,
                RELATIVE,
                &format!("roof corner moved {signed:e} mm"),
            );
        }
    }
}

/// The corpus Cylinder's defect, rebuilt from a fixture: a 61-gon prism,
/// 30 mm across and 30 mm long, stored as f32 and jittered the way Lane 0b
/// jittered it. Every side pose lays one side facet down (and its opposite
/// facet roughly up), so every side pose is congruent and must measure one
/// peak, inside the prism's own 30 x 30 mm bounding section.
#[test]
fn congruent_side_poses_of_a_cylinder_measure_one_peak() {
    let segments = 61_usize;
    let prism = make_disc_fan(30.0, 30.0, segments).expect("61-gon prism");
    let models = [
        ("stored", as_stored(&prism)),
        ("+1e-9 rad, +1 ulp", jittered(&prism, 1e-9, 1.0)),
        ("+1e-9 rad, -1 ulp", jittered(&prism, 1e-9, -1.0)),
        ("-3e-9 rad, -1 ulp", jittered(&prism, -3e-9, -1.0)),
    ];
    // The widest chord of a regular N-gon is at least `D cos(pi / 2N)`.
    #[expect(clippy::cast_precision_loss, reason = "a small fixture segment count")]
    let lower = 30.0 * 30.0 * (std::f64::consts::PI / (2.0 * segments as f64)).cos();
    for (name, mesh) in models {
        // The rest-pose directions the grid derives: each side facet's
        // outward normal and its reverse, from the stored vertices.
        let mut directions = Vec::new();
        for triangle in mesh.tris() {
            let [a, b, c] = triangle.map(|corner| mesh.verts()[corner as usize]);
            let normal = vec3::normalize(vec3::cross(vec3::sub(b, a), vec3::sub(c, a)))
                .expect("non-degenerate facet");
            if normal[2].abs() < 1e-3 {
                directions.push(normal);
                directions.push(normal.map(|coordinate| -coordinate));
            }
        }
        assert_eq!(
            directions.len(),
            4 * segments,
            "{name}: two facets per side"
        );
        let measured = peaks(&mesh, &directions);
        let (low, high) = measured
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), peak| {
                (low.min(*peak), high.max(*peak))
            });
        // f32 storage moves each vertex by up to ~1e-6 mm, so the stored poses
        // are congruent only to ~1e-7 of their section (measured: 4e-8).
        assert!(
            high - low <= RELATIVE * high,
            "{name}: congruent side poses measured {low} to {high} mm²"
        );
        assert!(
            low >= lower * (1.0 - RELATIVE) && high <= 900.0 * (1.0 + RELATIVE),
            "{name}: side peak {low}..{high} mm² outside the prism's real section [{lower}, 900]"
        );
    }
}
