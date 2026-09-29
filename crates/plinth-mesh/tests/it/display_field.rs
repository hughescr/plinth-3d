//! **What the display-face field says about geometry whose answer is known.**
//!
//! Five shapes, built here rather than loaded, each chosen because the right
//! answer is a fact about the shape and not a judgement about it:
//!
//! * a **recessed disc** in the `Round 25` proportions, whose wide flat glue
//!   face is invisible from the authored upper hemisphere in one orientation
//!   of the source file and fully visible in the mirror of it;
//! * a **two-sided textured plate**, bumped on BOTH authored faces, which is
//!   the Morel base's problem in miniature: detail density cannot tell the two
//!   sides apart and only the authored up-signal can;
//! * a **slender peg with a rounded tip**, the confound the owner named —
//!   sharply varying normals that are the BEST place to land a support;
//! * a **textured slab**, carving that must NOT be discounted;
//! * a **regular groove field**, one fold angle repeated, which is CHANGE and
//!   not detail (owner ruling 2026-09-05, built 2026-09-28).
//!
//! Every assertion below is a comparison between two measurements of these
//! shapes. There is no absolute magnitude asserted anywhere except `0`, which
//! the visibility factor produces by construction rather than by rounding.

use plinth_mesh::{
    evaluate_display, DisplayBuildConfig, DisplayField, DisplaySettings, Mesh,
};

fn settings() -> DisplaySettings {
    DisplaySettings::production().expect("production display settings")
}

fn field(mesh: &Mesh) -> DisplayField {
    let config = DisplayBuildConfig::production(mesh.tris().len()).expect("display config");
    evaluate_display(mesh, "test-lod", settings(), config).expect("display field")
}

/// Per-face outward unit normal and area, recomputed here so a test never
/// agrees with the evaluator by sharing its arithmetic.
fn face_geometry(mesh: &Mesh) -> (Vec<f64>, Vec<f64>) {
    let verts = mesh.verts();
    let mut normal_z = Vec::with_capacity(mesh.tris().len());
    let mut areas = Vec::with_capacity(mesh.tris().len());
    for triangle in mesh.tris() {
        let a = verts[triangle[0] as usize];
        let b = verts[triangle[1] as usize];
        let c = verts[triangle[2] as usize];
        let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let cross = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let length = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
        areas.push(0.5 * length);
        normal_z.push(if length > 0.0 { cross[2] / length } else { 0.0 });
    }
    (normal_z, areas)
}

/// Area-weighted mean of D over the faces a predicate selects, and the count
/// of them, so a zero mean over an empty set can never be mistaken for a
/// measurement.
fn mean_display(mesh: &Mesh, field: &DisplayField, select: impl Fn(usize, f64) -> bool) -> (f64, usize) {
    let (normal_z, areas) = face_geometry(mesh);
    let mut area = 0.0_f64;
    let mut weighted = 0.0_f64;
    let mut count = 0_usize;
    for (face, value) in field.face_values().iter().enumerate() {
        if select(face, normal_z[face]) {
            area += areas[face];
            weighted += areas[face] * value.weight();
            count += 1;
        }
    }
    (if area > 0.0 { weighted / area } else { 0.0 }, count)
}

/// Mirror a mesh through `z = 0`, reversing each triangle's winding so the
/// result is the same solid the other way up with its outward normals still
/// outward. Used to ask the same question about the same shape in the opposite
/// AUTHORED frame.
fn mirror_z(mesh: &Mesh) -> Mesh {
    let verts: Vec<[f64; 3]> = mesh
        .verts()
        .iter()
        .map(|vertex| [vertex[0], vertex[1], -vertex[2]])
        .collect();
    let tris: Vec<[u32; 3]> = mesh
        .tris()
        .iter()
        .map(|triangle| [triangle[0], triangle[2], triangle[1]])
        .collect();
    Mesh::new(verts, tris).expect("a mirrored mesh is still a mesh")
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// Index a fixture vertex, refusing rather than wrapping. These fixtures are
/// thousands of vertices, not billions; a failure here is a fixture bug.
fn index(value: usize) -> u32 {
    u32::try_from(value).expect("a fixture vertex index fits a u32")
}

/// A closed rectangular slab whose top and bottom surfaces are each displaced
/// by a caller-supplied height field, on a shared `(cells + 1)²` lattice.
///
/// `half_thickness_mm` is the unbumped half-thickness; each displacement is
/// added outward, so a bump function returning `0` reproduces a plain slab.
/// The four side walls close the solid, so the surface is a manifold and the
/// welded angle defect is meaningful everywhere except the four vertical
/// corner seams.
fn bumped_plate(
    span_mm: f64,
    half_thickness_mm: f64,
    cells: usize,
    top: impl Fn(f64, f64) -> f64,
    bottom: impl Fn(f64, f64) -> f64,
) -> Mesh {
    let step = span_mm / cells as f64;
    let coordinate = |index: usize| -span_mm / 2.0 + step * index as f64;
    let stride = cells + 1;
    let mut verts: Vec<[f64; 3]> = Vec::with_capacity(2 * stride * stride);
    for row in 0..stride {
        for column in 0..stride {
            let (x, y) = (coordinate(column), coordinate(row));
            verts.push([x, y, half_thickness_mm + top(x, y)]);
        }
    }
    for row in 0..stride {
        for column in 0..stride {
            let (x, y) = (coordinate(column), coordinate(row));
            verts.push([x, y, -half_thickness_mm - bottom(x, y)]);
        }
    }
    let upper = |row: usize, column: usize| index(row * stride + column);
    let base = stride * stride;
    let lower = |row: usize, column: usize| index(base + row * stride + column);
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for row in 0..cells {
        for column in 0..cells {
            // Top surface, outward normal +z.
            tris.push([
                upper(row, column),
                upper(row, column + 1),
                upper(row + 1, column + 1),
            ]);
            tris.push([
                upper(row, column),
                upper(row + 1, column + 1),
                upper(row + 1, column),
            ]);
            // Bottom surface, outward normal -z (reversed winding).
            tris.push([
                lower(row, column),
                lower(row + 1, column + 1),
                lower(row, column + 1),
            ]);
            tris.push([
                lower(row, column),
                lower(row + 1, column),
                lower(row + 1, column + 1),
            ]);
        }
    }
    // Four walls, each stitched between the top and bottom rims.
    for index in 0..cells {
        // y = -span/2 edge (row 0), outward -y.
        tris.push([upper(0, index), lower(0, index), lower(0, index + 1)]);
        tris.push([upper(0, index), lower(0, index + 1), upper(0, index + 1)]);
        // y = +span/2 edge (row `cells`), outward +y.
        tris.push([
            upper(cells, index),
            upper(cells, index + 1),
            lower(cells, index + 1),
        ]);
        tris.push([
            upper(cells, index),
            lower(cells, index + 1),
            lower(cells, index),
        ]);
        // x = -span/2 edge (column 0), outward -x.
        tris.push([
            upper(index, 0),
            upper(index + 1, 0),
            lower(index + 1, 0),
        ]);
        tris.push([upper(index, 0), lower(index + 1, 0), lower(index, 0)]);
        // x = +span/2 edge (column `cells`), outward +x.
        tris.push([upper(index, cells), lower(index, cells), lower(index + 1, cells)]);
        tris.push([
            upper(index, cells),
            lower(index + 1, cells),
            upper(index + 1, cells),
        ]);
    }
    Mesh::new(verts, tris).expect("the bumped plate is a valid mesh")
}

/// A slender vertical peg of radius `radius_mm` standing on a closed base
/// disc, capped by a hemisphere: the owner's named confound, built to scale
/// (a rivet nub / fingertip, not a spire).
fn capped_peg(radius_mm: f64, shaft_mm: f64, segments: usize, rings: usize) -> Mesh {
    let mut verts: Vec<[f64; 3]> = Vec::new();
    let angle = |index: usize| std::f64::consts::TAU * index as f64 / segments as f64;
    // 0: the base centre.
    verts.push([0.0, 0.0, 0.0]);
    // Base rim, then shaft top rim.
    for index in 0..segments {
        let (sin, cos) = angle(index).sin_cos();
        verts.push([radius_mm * cos, radius_mm * sin, 0.0]);
    }
    for index in 0..segments {
        let (sin, cos) = angle(index).sin_cos();
        verts.push([radius_mm * cos, radius_mm * sin, shaft_mm]);
    }
    // Hemispherical cap rings, then the apex.
    for ring in 1..rings {
        let polar = std::f64::consts::FRAC_PI_2 * ring as f64 / rings as f64;
        let (sin, cos) = polar.sin_cos();
        for index in 0..segments {
            let (s, c) = angle(index).sin_cos();
            verts.push([
                radius_mm * cos * c,
                radius_mm * cos * s,
                shaft_mm + radius_mm * sin,
            ]);
        }
    }
    let apex = index(verts.len());
    verts.push([0.0, 0.0, shaft_mm + radius_mm]);

    let base_rim = |at: usize| index(1 + at % segments);
    let shaft_rim = |at: usize| index(1 + segments + at % segments);
    let cap_ring = |ring: usize, at: usize| {
        index(1 + 2 * segments + (ring - 1) * segments + at % segments)
    };

    let mut tris: Vec<[u32; 3]> = Vec::new();
    for index in 0..segments {
        // Base fan, outward -z.
        tris.push([0, base_rim(index + 1), base_rim(index)]);
        // Shaft wall.
        tris.push([base_rim(index), base_rim(index + 1), shaft_rim(index + 1)]);
        tris.push([base_rim(index), shaft_rim(index + 1), shaft_rim(index)]);
    }
    for ring in 1..rings {
        for index in 0..segments {
            let (below, above) = if ring == 1 {
                (shaft_rim(index), cap_ring(1, index))
            } else {
                (cap_ring(ring - 1, index), cap_ring(ring, index))
            };
            let (below_next, above_next) = if ring == 1 {
                (shaft_rim(index + 1), cap_ring(1, index + 1))
            } else {
                (cap_ring(ring - 1, index + 1), cap_ring(ring, index + 1))
            };
            tris.push([below, below_next, above_next]);
            tris.push([below, above_next, above]);
        }
    }
    for index in 0..segments {
        tris.push([cap_ring(rings - 1, index), cap_ring(rings - 1, index + 1), apex]);
    }
    Mesh::new(verts, tris).expect("the capped peg is a valid mesh")
}

// ---------------------------------------------------------------------------
// (b) the Round 25 disc, both ways up in the AUTHORED file
// ---------------------------------------------------------------------------

/// **A face the authored upper hemisphere cannot see scores exactly zero, and
/// the same solid mirrored gives the mirror answer.**
///
/// The fixture is the `Round 25` geometry `round_bases_ratchet.rs` grades, in
/// its authored handedness: measured on `corpus/models/Round_25.stl`, the
/// stiffener ring sits at the authored `z` MAXIMUM and the wide flat glue face
/// at the authored `z` minimum. (That is worth stating because the owner's
/// 2026-09-03 ChituBox verdict is about the PRINTED file, where the ring is
/// down — the engine gets there by flipping the authored mesh 180°.)
///
/// So this test asserts the shape of the field, in both authored frames:
///
/// * the face pointing away from the authored upper hemisphere scores exactly
///   `0`, whichever face that is;
/// * the face pointing INTO it is SEEN (visibility above `0`) and still
///   scores `D = 0`, because it is featureless: the only fold its
///   neighbourhood holds is the rim crease, one angle, which is change and
///   not detail (owner ruling 2026-09-05, built 2026-09-28).
///
/// **Amended 2026-09-28.** Until the spread statistic replaced the mean, this
/// test asserted the opposite of the second bullet — that the up-facing glue
/// face "should carry some display value" because the drafted rim
/// articulates. That was the mean statistic reading a crease as detail, the
/// very reading that charged the Round bases' correct glue-face-up pose. The
/// featureless glue face IS the display face; no detail measure can see that
/// positively, and D must at least not push against it.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "both claims ARE exact zero: `max(0, n·v)` is zero for every upper-hemisphere `v` \
              when `n` points straight down, and a spread over one fold angle is `1 - GM/AM` of \
              identical values, so a tolerance would pass on a field that had stopped producing \
              either by construction"
)]
fn a_recessed_disc_scores_zero_on_the_face_the_authored_top_cannot_see() {
    let authored = plinth_mesh::make_recessed_disc(11.5, 12.5, 3.5, 2.5, 10.0, 96)
        .expect("the Round 25 disc profile");
    let mirrored = mirror_z(&authored);

    for (name, mesh, glue_face_z) in [
        ("authored (ring up, glue face DOWN)", &authored, -1.0_f64),
        ("mirrored (ring down, glue face UP)", &mirrored, 1.0_f64),
    ] {
        let measured = field(mesh);
        let (normal_z, _) = face_geometry(mesh);
        let values = measured.face_values();

        let (glue_mean, glue_faces) =
            mean_display(mesh, &measured, |_, nz| nz * glue_face_z > 0.99);
        let glue_max = values
            .iter()
            .enumerate()
            .filter(|(face, _)| normal_z[*face] * glue_face_z > 0.99)
            .map(|(_, value)| value.weight())
            .fold(0.0_f64, f64::max);
        assert!(glue_faces > 0, "{name}: no glue-face triangles were selected");

        if glue_face_z < 0.0 {
            // Facing away from the authored upper hemisphere: EXACTLY zero,
            // not merely small. `max(0, n·v)` is zero for every `v` above the
            // equator when `n` points straight down, so there is nothing to
            // round.
            assert_eq!(
                glue_max, 0.0,
                "{name}: a downward-facing face scored {glue_max}, which the visibility \
                 construction cannot produce"
            );
        } else {
            let seen = values
                .iter()
                .enumerate()
                .filter(|(face, _)| normal_z[*face] * glue_face_z > 0.99)
                .map(|(_, value)| value.visibility())
                .fold(0.0_f64, f64::max);
            println!(
                "{name}: glue face mean D {glue_mean:.6}, max D {glue_max:.6}, max visibility \
                 {seen:.6}"
            );
            assert!(
                seen > 0.0,
                "{name}: the authored-up glue face is not visible from the authored upper \
                 hemisphere, so this assertion cannot separate detail from visibility"
            );
            assert_eq!(
                glue_max, 0.0,
                "{name}: the featureless glue face scored D {glue_max}; its neighbourhoods hold \
                 one fold angle (the rim crease), which is change, not detail"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// (c) the two-sided plate: the Morel base's problem, in miniature
// ---------------------------------------------------------------------------

/// **Detail on BOTH authored faces, and the authored up-signal is what
/// separates them.**
///
/// This is the Morel base reduced to its essentials: the ground texture on the
/// authored top and the raised relief text on the authored bottom BOTH
/// articulate, so no detail measure alone can rank them, and the engine's
/// wrong 177° pose put the textured side against the plate. Here both faces
/// carry the SAME bump field — the two are geometrically indistinguishable
/// except for which way they face — so any separation the field reports is
/// entirely the visibility factor's.
///
/// Sabotage (recorded 2026-09-04, `plinth-evidence/.../display-face/`):
/// forcing `visibility` to `1.0` in `display_eval.rs` turns the ratio below to
/// ~1 and reddens this assertion. That is the drill that proves the factor is
/// load-bearing here rather than merely present.
#[test]
fn a_plate_textured_on_both_sides_is_separated_by_the_authored_up_signal() {
    let bumps = |x: f64, y: f64| {
        0.18 * ((x * 2.4).sin() * (y * 2.4).sin()).abs()
    };
    let plate = bumped_plate(12.0, 1.5, 60, bumps, bumps);
    let measured = field(&plate);

    let (top, top_faces) = mean_display(&plate, &measured, |_, nz| nz > 0.5);
    let (bottom, bottom_faces) = mean_display(&plate, &measured, |_, nz| nz < -0.5);
    println!(
        "two-sided plate: authored-top D {top:.6} over {top_faces} faces, authored-bottom D \
         {bottom:.6} over {bottom_faces} faces"
    );
    assert!(top_faces > 100 && bottom_faces > 100, "the plate fixture is too coarse to grade");
    assert!(
        top > 10.0 * bottom.max(f64::MIN_POSITIVE),
        "authored-top D {top:.6} is not >> authored-bottom D {bottom:.6}; the two faces carry \
         the same texture, so anything but a large separation means the authored up-signal is \
         not deciding this"
    );
}

// ---------------------------------------------------------------------------
// (c2) uniform change is not detail (owner ruling 2026-09-05, built 2026-09-28)
// ---------------------------------------------------------------------------

/// **A regular groove field is CHANGE, not detail; an irregular texture is
/// detail.**
///
/// Owner ruling 2026-09-05: "detail" is a WIDE DISTRIBUTION of normal-change
/// magnitudes in a neighbourhood — small, medium and large together. A
/// crease, a regular rib, a regular groove is one repeated change, however
/// sharp, and is not detail. The Round bases are where the old reading did
/// harm: their stiffener ribs and rim edges read as detail and charged the
/// correct glue-face-up pose 24.1 mm2 on Round 100 (`round50.md` Q3).
///
/// Two surfaces, both fully visible from the authored upper hemisphere and
/// neither closed over into a tip, so visibility and the tip discount cannot
/// be what separates them:
///
/// * regular V grooves, 1 mm pitch, every fold the same angle — CHANGE;
/// * the `|sin x sin y|` bump field the other tests here call texture, whose
///   folds run from shallow flanks to sharp creases — DETAIL.
///
/// Under the mean-articulation reading this ran the wrong way: the grooves
/// articulate harder than the bumps, so they read as MORE detail.
#[test]
fn a_regular_groove_field_is_change_and_an_irregular_texture_is_detail() {
    let grooves = bumped_plate(
        12.0,
        1.5,
        120,
        |x, _| 0.12 * (1.0 - ((x.rem_euclid(1.0)) - 0.5).abs() * 2.0),
        |_, _| 0.0,
    );
    let texture = bumped_plate(
        12.0,
        1.5,
        120,
        |x, y| 0.18 * ((x * 2.4).sin() * (y * 2.4).sin()).abs(),
        |_, _| 0.0,
    );
    let groove_field = field(&grooves);
    let texture_field = field(&texture);
    let (groove_d, groove_faces) = mean_display(&grooves, &groove_field, |_, nz| nz > 0.5);
    let (texture_d, texture_faces) = mean_display(&texture, &texture_field, |_, nz| nz > 0.5);
    println!(
        "regular grooves: D {groove_d:.6} over {groove_faces} faces; irregular texture: D \
         {texture_d:.6} over {texture_faces} faces"
    );
    assert!(groove_faces > 100 && texture_faces > 100, "a fixture is too coarse to grade");
    assert!(texture_d > 0.0, "the irregular texture must carry detail, or the comparison is vacuous");
    assert!(
        groove_d < 0.1 * texture_d,
        "regular grooves scored D {groove_d:.6} against the irregular texture's {texture_d:.6}; a \
         field of one repeated fold angle is change, not detail, and must read well below a \
         texture whose folds span shallow to sharp"
    );
}

// ---------------------------------------------------------------------------
// (d) the tip confound against real carving
// ---------------------------------------------------------------------------

/// **A protrusion tip is discounted; a carved texture is not.**
///
/// The peg's rounded cap is the owner's named confound: normals that vary
/// sharply and a place a support SHOULD land. The textured slab is carving:
/// normals that vary just as sharply and a place a support should not land.
/// Both are fully visible from the authored upper hemisphere, so visibility
/// cannot be what separates them — the tip discount is.
///
/// Two assertions, because "low" alone would pass for the wrong reason: the
/// discount must actually FIRE on the cap (`tip_factor < 1`, a path-hit
/// witness), and the textured slab must end up ranked above it.
///
/// **Amended 2026-09-28.** The carving here used to be a REGULAR 1 mm V-groove
/// field. Under the owner's 2026-09-05 ruling a regular groove is change, not
/// detail, and `a_regular_groove_field_is_change_and_an_irregular_texture_is_detail`
/// now asserts exactly that about the same grooves, so they can no longer
/// stand for carving here. The irregular bump texture the other tests in
/// this file already call texture takes their place.
#[test]
fn a_rounded_tip_is_discounted_and_a_carved_texture_is_not() {
    let peg = capped_peg(0.35, 1.2, 48, 12);
    let peg_field = field(&peg);
    let cap_z = 1.2_f64;
    let (peg_tip_mean, peg_tip_faces) = mean_display(&peg, &peg_field, |face, _| {
        let verts = peg.verts();
        let triangle = peg.tris()[face];
        (0..3).all(|corner| verts[triangle[corner] as usize][2] >= cap_z)
    });
    let tip_factor_min = peg_field
        .face_values()
        .iter()
        .enumerate()
        .filter(|(face, _)| {
            let verts = peg.verts();
            let triangle = peg.tris()[*face];
            (0..3).all(|corner| verts[triangle[corner] as usize][2] >= cap_z)
        })
        .map(|(_, value)| value.tip_factor())
        .fold(1.0_f64, f64::min);

    // The irregular bump texture on the authored top of a thick slab: a
    // near-planar patch whose crests and troughs cancel in total curvature,
    // so the tip discount has little to fire on, while the fold angles run
    // from shallow flanks to sharp creases.
    let texture = bumped_plate(
        12.0,
        1.5,
        120,
        |x, y| 0.18 * ((x * 2.4).sin() * (y * 2.4).sin()).abs(),
        |_, _| 0.0,
    );
    let texture_field = field(&texture);
    let (texture_mean, texture_faces) = mean_display(&texture, &texture_field, |_, nz| nz > 0.5);

    println!(
        "peg cap: D {peg_tip_mean:.6} over {peg_tip_faces} faces, min tip_factor \
         {tip_factor_min:.6}; carved texture: D {texture_mean:.6} over {texture_faces} faces"
    );
    assert!(peg_tip_faces > 20 && texture_faces > 100, "a fixture is too coarse to grade");
    assert!(
        tip_factor_min < 1.0,
        "the tip discount never fired on the peg's rounded cap (min tip_factor \
         {tip_factor_min}); this assertion is the path-hit witness, so a passing D comparison \
         without it would prove nothing"
    );
    assert!(
        texture_mean > peg_tip_mean,
        "the carved texture scored D {texture_mean:.6}, at or below the peg tip's \
         {peg_tip_mean:.6}; both are fully visible from above, so this comparison is the tip \
         discount's whole job"
    );
}

// ---------------------------------------------------------------------------
// (f) occlusion: an authored-top face hidden under other geometry
// ---------------------------------------------------------------------------

/// Two closed solids in one mesh: `lower` as built, and `roof` translated by
/// `offset`, with the roof's indices rebased onto the merged vertex list.
fn stacked(lower: &Mesh, roof: &Mesh, offset: [f64; 3]) -> Mesh {
    let mut verts: Vec<[f64; 3]> = lower.verts().to_vec();
    let base = index(verts.len());
    verts.extend(roof.verts().iter().map(|v| [v[0] + offset[0], v[1] + offset[1], v[2] + offset[2]]));
    let mut tris: Vec<[u32; 3]> = lower.tris().to_vec();
    tris.extend(roof.tris().iter().map(|t| [t[0] + base, t[1] + base, t[2] + base]));
    Mesh::new(verts, tris).expect("the stacked pair is a valid mesh")
}

/// **A textured top face under a roof is not a display face.**
///
/// The textured plate is mirror-symmetric in `x`, so its two halves carry the
/// same detail and the same tip factor; the only thing the roof over the
/// `x > 0` half changes is what the authored upper hemisphere can SEE. If the
/// occlusion half of the visibility factor were deleted or inverted, the two
/// halves would score alike and this test would go red — the earlier tests
/// separate faces by normal sign alone and cannot tell.
#[test]
fn a_textured_face_under_a_roof_loses_its_display_value() {
    let bumps = |x: f64, y: f64| 0.18 * ((x * 2.4).sin() * (y * 2.4).sin()).abs();
    let flat = |_: f64, _: f64| 0.0;
    let plate = bumped_plate(12.0, 1.5, 60, bumps, bumps);
    // A plain slab 12 mm square, 1 mm thick, hovering 0.5 mm above the plate's
    // highest bump (1.68 mm) and covering the x > 0 half only (x in [0, 12]).
    let roof = bumped_plate(12.0, 0.5, 4, flat, flat);
    let mesh = stacked(&plate, &roof, [6.0, 0.0, 2.7]);
    let measured = field(&mesh);

    let verts = mesh.verts();
    let (normal_z, areas) = face_geometry(&mesh);
    let mut covered = (0.0_f64, 0.0_f64, 0_usize);
    let mut exposed = (0.0_f64, 0.0_f64, 0_usize);
    for (face, value) in measured.face_values().iter().enumerate() {
        let t = mesh.tris()[face];
        let cx = (verts[t[0] as usize][0] + verts[t[1] as usize][0] + verts[t[2] as usize][0]) / 3.0;
        let cz = (verts[t[0] as usize][2] + verts[t[1] as usize][2] + verts[t[2] as usize][2]) / 3.0;
        // The lower plate's top surface only, away from the roof's edge.
        if cz > 2.5 || normal_z[face] < 0.5 || cx.abs() < 1.5 {
            continue;
        }
        let bucket = if cx > 0.0 { &mut covered } else { &mut exposed };
        bucket.0 += areas[face];
        bucket.1 += areas[face] * value.weight();
        bucket.2 += 1;
    }
    let covered_d = covered.1 / covered.0;
    let exposed_d = exposed.1 / exposed.0;
    println!(
        "roofed plate: covered-half D {covered_d:.6} over {} faces, exposed-half D {exposed_d:.6} \
         over {} faces",
        covered.2, exposed.2
    );
    assert!(covered.2 > 100 && exposed.2 > 100, "the roofed fixture is too coarse to grade");
    assert!(exposed_d > 0.0, "the exposed half must carry detail, or the comparison is vacuous");
    assert!(
        covered_d < 0.5 * exposed_d,
        "covered-half D {covered_d:.6} is not well below exposed-half D {exposed_d:.6}; the two \
         halves differ only in what the roof hides, so occlusion is not being counted"
    );
}
