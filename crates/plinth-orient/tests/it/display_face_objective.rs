//! **The display-face field inside the orientation objective.**
//!
//! Two properties, and the second is the one that makes the first safe to
//! land:
//!
//! * the field is measured ONCE, in the mesh's AUTHORED frame, and is CARRIED
//!   into every candidate pose by face identity — never recomputed in a posed
//!   frame;
//! * where the field says nothing (`D = 0` over the faces a pose turns
//!   downward), the objective is the pre-display objective BIT FOR BIT, not
//!   approximately.
//!
//! The scar term is
//!
//! ```text
//! scar_raw_mm2 = Σ_down(area_mm² × (fragility + D))
//!              = fragility_raw_mm2 + display_raw_mm2
//! ```
//!
//! with `fragility` the delicacy field, unchanged, and no new weight anywhere:
//! `ORIENT_SCAR_WEIGHT` still prices the whole term and `D` enters as a
//! dimensionless per-area harm BESIDE fragility, not as a multiplier on it
//! (2026-09-29; test (f) below). Fragility and display are published
//! separately and all of it is asserted here.

use plinth_mesh::Mesh;
use plinth_orient::{
    score_candidates, CandidateOutcome, CandidateTerms, ObjectiveWeights, OrientFaceTable,
    OrientSearchLimits, UpDirection,
};

fn score(table: &OrientFaceTable, candidates: &[UpDirection]) -> Vec<CandidateTerms> {
    let weights = ObjectiveWeights::registered().expect("registered objective weights");
    let scored = score_candidates(table, candidates, weights, OrientSearchLimits::production())
        .expect("scoring must not refuse these fixtures");
    scored
        .outcomes()
        .iter()
        .map(|outcome| match outcome {
            CandidateOutcome::Scored(terms) => *terms,
            CandidateOutcome::Refused(refusal) => panic!("candidate refused: {refusal:?}"),
        })
        .collect()
}

fn up(direction: [f64; 3]) -> UpDirection {
    UpDirection::new(direction).expect("a unit build direction")
}

/// Index a fixture vertex, refusing rather than wrapping. These fixtures are
/// thousands of vertices, not billions; a failure here is a fixture bug.
fn index(value: usize) -> u32 {
    u32::try_from(value).expect("a fixture vertex index fits a u32")
}

/// A closed slab, bumped identically on BOTH authored faces: the Morel base's
/// problem in miniature. Detail density cannot tell the two sides apart, so
/// anything that does is the authored up-signal.
fn two_sided_plate() -> Mesh {
    let bump = |x: f64, y: f64| 0.18 * ((x * 2.4).sin() * (y * 2.4).sin()).abs();
    let (span, half_thickness, cells) = (12.0_f64, 1.5_f64, 48_usize);
    let step = span / cells as f64;
    let coordinate = |index: usize| -span / 2.0 + step * index as f64;
    let stride = cells + 1;
    let mut verts: Vec<[f64; 3]> = Vec::with_capacity(2 * stride * stride);
    for row in 0..stride {
        for column in 0..stride {
            let (x, y) = (coordinate(column), coordinate(row));
            verts.push([x, y, half_thickness + bump(x, y)]);
        }
    }
    for row in 0..stride {
        for column in 0..stride {
            let (x, y) = (coordinate(column), coordinate(row));
            verts.push([x, y, -half_thickness - bump(x, y)]);
        }
    }
    let upper = |row: usize, column: usize| index(row * stride + column);
    let base = stride * stride;
    let lower = |row: usize, column: usize| index(base + row * stride + column);
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for row in 0..cells {
        for column in 0..cells {
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
    for index in 0..cells {
        tris.push([upper(0, index), lower(0, index), lower(0, index + 1)]);
        tris.push([upper(0, index), lower(0, index + 1), upper(0, index + 1)]);
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
        tris.push([upper(index, 0), upper(index + 1, 0), lower(index + 1, 0)]);
        tris.push([upper(index, 0), lower(index + 1, 0), lower(index, 0)]);
        tris.push([
            upper(index, cells),
            lower(index, cells),
            lower(index + 1, cells),
        ]);
        tris.push([
            upper(index, cells),
            lower(index + 1, cells),
            upper(index + 1, cells),
        ]);
    }
    Mesh::new(verts, tris).expect("the two-sided plate is a valid mesh")
}

/// **(a) D is carried into the pose by face identity, never recomputed in the
/// posed frame.**
///
/// The demand the owner's ruling makes is NOT that D be rotation invariant —
/// it is emphatically not, and must not be, because the whole signal is which
/// way the AUTHORED file points. The demand is that the value measured once on
/// the source mesh is the value every candidate pose reads. So the assertion
/// is made on ONE table, scored at two opposite build directions: the display
/// increment must move with which face set the pose turns downward, and the
/// two faces of this plate carry the same texture, so the only thing that can
/// separate them is the authored frame the field was measured in.
///
/// The mechanism that makes this structural rather than lucky is stated in
/// `plinth-mesh/src/display.rs`: the scoring table is orientation invariant,
/// the sweep never rotates a mesh, and `score_one` reads `table.display[face]`
/// by index. There is no constructor that could hand this table a field
/// measured after a rotation.
#[test]
fn the_display_field_is_measured_once_in_the_authored_frame_and_carried_into_every_pose() {
    let plate = two_sided_plate();
    let table = OrientFaceTable::build(&plate, "test-lod").expect("scoring table");
    assert!(
        table.display_digest().is_some(),
        "a table built for a scar-weighted objective must carry a display field"
    );

    let terms = score(&table, &[up([0.0, 0.0, 1.0]), up([0.0, 0.0, -1.0])]);
    let (authored_up, authored_down) = (terms[0], terms[1]);

    // Sanity: the two poses turn down the same AMOUNT of geometry, because the
    // plate is symmetric. Anything that differs between them is the field.
    let fragility_ratio = authored_down.fragility_raw_mm2 / authored_up.fragility_raw_mm2;
    assert!(
        (0.9..=1.1).contains(&fragility_ratio),
        "the fixture is not symmetric in fragility ({:.4} vs {:.4}); the comparison below would \
         then be measuring the fixture, not the field",
        authored_up.fragility_raw_mm2,
        authored_down.fragility_raw_mm2,
    );

    println!(
        "u = +Z (authored BOTTOM faces down): fragility {:.4} display {:.4}\n\
         u = -Z (authored TOP faces down):    fragility {:.4} display {:.4}",
        authored_up.fragility_raw_mm2,
        authored_up.display_raw_mm2,
        authored_down.fragility_raw_mm2,
        authored_down.display_raw_mm2,
    );
    assert!(
        authored_down.display_raw_mm2 > 10.0 * authored_up.display_raw_mm2.max(f64::MIN_POSITIVE),
        "turning the AUTHORED TOP downward cost {:.6} mm2 of display scar against {:.6} for \
         turning the authored bottom down; the two faces are identically textured, so without a \
         large separation the authored frame is not reaching the objective",
        authored_down.display_raw_mm2,
        authored_up.display_raw_mm2,
    );

    // The published halves must add up to the published whole, on both poses.
    for terms in [authored_up, authored_down] {
        assert!(
            (terms.scar_raw_mm2 - (terms.fragility_raw_mm2 + terms.display_raw_mm2)).abs()
                <= f64::EPSILON * terms.scar_raw_mm2.abs().max(1.0),
            "scar_raw_mm2 {} is not fragility {} + display {}",
            terms.scar_raw_mm2,
            terms.fragility_raw_mm2,
            terms.display_raw_mm2,
        );
    }
}

/// A closed slab with an irregularly bumped AUTHORED TOP and a plain flat
/// bottom at `z = 0`, `thickness_mm` thick under the bumps. The top is the
/// display face; the bottom carries no detail. Only the thickness varies
/// between the two slabs the test below compares, and thickness moves the
/// top's FRAGILITY (the delicacy field's thinness proxy saturates at
/// `THIN_FEATURE_MM`, 1.5 mm) and nothing the display field measures.
///
/// The texture stops 2 mm inside the rim, more than twice `R_detail`
/// (0.8 mm), so no detail neighbourhood of the texture reaches the side walls
/// (whose shape does change with thickness), and the rim's own neighbourhoods
/// see only its uniform 90° creases, which rule B scores `D = 0` on both slabs.
fn textured_top_slab(thickness_mm: f64) -> Mesh {
    let bump = |x: f64, y: f64| {
        if x.abs() > 4.0 || y.abs() > 4.0 {
            0.0
        } else {
            0.18 * ((x * 2.4).sin() * (y * 1.7).sin()).abs()
        }
    };
    let (span, cells) = (12.0_f64, 48_usize);
    let step = span / cells as f64;
    let coordinate = |index: usize| -span / 2.0 + step * index as f64;
    let stride = cells + 1;
    let mut verts: Vec<[f64; 3]> = Vec::with_capacity(2 * stride * stride);
    for row in 0..stride {
        for column in 0..stride {
            let (x, y) = (coordinate(column), coordinate(row));
            verts.push([x, y, thickness_mm + bump(x, y)]);
        }
    }
    for row in 0..stride {
        for column in 0..stride {
            verts.push([coordinate(column), coordinate(row), 0.0]);
        }
    }
    let upper = |row: usize, column: usize| index(row * stride + column);
    let base = stride * stride;
    let lower = |row: usize, column: usize| index(base + row * stride + column);
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for row in 0..cells {
        for column in 0..cells {
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
    for index in 0..cells {
        tris.push([upper(0, index), lower(0, index), lower(0, index + 1)]);
        tris.push([upper(0, index), lower(0, index + 1), upper(0, index + 1)]);
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
        tris.push([upper(index, 0), upper(index + 1, 0), lower(index + 1, 0)]);
        tris.push([upper(index, 0), lower(index + 1, 0), lower(index, 0)]);
        tris.push([
            upper(index, cells),
            lower(index, cells),
            lower(index + 1, cells),
        ]);
        tris.push([
            upper(index, cells),
            lower(index + 1, cells),
            upper(index + 1, cells),
        ]);
    }
    Mesh::new(verts, tris).expect("the textured-top slab is a valid mesh")
}

/// **(f) A support spoils the look of a display face however sturdy that face
/// is: the display harm does not scale with fragility.**
///
/// `plinth-mesh/src/display.rs` defines the two quantities apart: delicacy
/// asks *would a support break this*, display asks *would a support spoil the
/// look of this*, and "neither is expressed in terms of the other". A scar on
/// a sturdy textured base top spoils it exactly as much as the same scar on a
/// thin one. So the display half of the scar term must not move when only the
/// face's fragility moves.
///
/// Before 2026-09-29 the display half was `Σ_down(area × fragility × D)`: the
/// display field entered as a `(1 + D)` MULTIPLIER on fragility, so a sturdy
/// display face was nearly unpriced. That is how the Morel base's textured top
/// (large, flat-ish, low fragility) went face down on develop `0f70cf2`: its
/// whole display charge was 6.685 mm² against a 21.3 mm² fragility saving
/// (`field-2026-09-28/lanes/morel-pose.md`).
///
/// The comparison: two slabs identical but for thickness under the textured
/// top, scored at `u = −Z` (textured top DOWN). The thin one's top is more
/// fragile; its display charge must be the same.
#[test]
fn a_display_face_costs_the_same_to_turn_down_however_sturdy_it_is() {
    let (thin, thick) = (textured_top_slab(0.9), textured_top_slab(4.0));
    let thin_table = OrientFaceTable::build(&thin, "test-lod").expect("thin scoring table");
    let thick_table = OrientFaceTable::build(&thick, "test-lod").expect("thick scoring table");
    let face_down = [up([0.0, 0.0, -1.0])];
    let thin_terms = score(&thin_table, &face_down)[0];
    let thick_terms = score(&thick_table, &face_down)[0];
    println!(
        "textured top DOWN: thin (0.9 mm) fragility {:.4} display {:.4}; thick (4.0 mm) \
         fragility {:.4} display {:.4}",
        thin_terms.fragility_raw_mm2,
        thin_terms.display_raw_mm2,
        thick_terms.fragility_raw_mm2,
        thick_terms.display_raw_mm2,
    );

    // Non-vacuity: the two slabs DO differ in fragility, by a lot, and the
    // texture IS display. Without both, equal display charges prove nothing.
    assert!(
        thin_terms.fragility_raw_mm2 > 1.5 * thick_terms.fragility_raw_mm2,
        "the thin slab's top is not clearly more fragile ({:.4} vs {:.4} mm2); the fixture no \
         longer separates fragility from display",
        thin_terms.fragility_raw_mm2,
        thick_terms.fragility_raw_mm2,
    );
    assert!(
        thick_terms.display_raw_mm2 > 0.0,
        "the textured top carries no display charge at all; the comparison below is vacuous"
    );

    let ratio = thin_terms.display_raw_mm2 / thick_terms.display_raw_mm2;
    assert!(
        (0.9..=1.1).contains(&ratio),
        "turning the same textured top down cost {:.4} mm2 of display on the thin slab and \
         {:.4} on the thick one (ratio {ratio:.3}); the display harm is following fragility, so a \
         sturdy display face is left unprotected",
        thin_terms.display_raw_mm2,
        thick_terms.display_raw_mm2,
    );
}

/// **(e) Where D is zero, the objective is the pre-display objective bit for
/// bit.**
///
/// The fixture is the `Round 25` disc in its authored handedness, and the pose
/// is identity, so the faces this pose turns downward are exactly the wide
/// flat glue face — which the authored upper hemisphere cannot see, so every
/// one of them scores `D = 0` by the visibility construction rather than by
/// rounding. Under that condition the display increment must be exactly `0.0`
/// and every published term must equal, BIT FOR BIT, the same term from a
/// table built with no display field at all.
///
/// Bitwise, not within a tolerance: the association order of the scar fold is
/// pinned precisely because a re-association is a different set of rounding
/// decisions and therefore, over a real candidate grid, potentially a
/// different argmin.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "the claim IS bit equality: a tolerance here would pass on exactly the re-association \
              defect the assertion exists to catch"
)]
fn a_zero_display_field_reproduces_the_previous_objective_bit_for_bit() {
    let disc = plinth_mesh::make_recessed_disc(11.5, 12.5, 3.5, 2.5, 10.0, 96)
        .expect("the Round 25 disc profile");
    let settings = plinth_mesh::DelicacySettings::production().expect("delicacy settings");
    let config = plinth_mesh::DelicacyBuildConfig::production(disc.tris().len());
    let fragility = plinth_mesh::evaluate_delicacy(&disc, "test-lod", settings, config)
        .expect("delicacy field");

    let with_display = OrientFaceTable::build(&disc, "test-lod").expect("table with display");
    let without_display = OrientFaceTable::with_delicacy(&disc, "test-lod", &fragility)
        .expect("table without display");
    assert!(with_display.display_digest().is_some());
    assert!(without_display.display_digest().is_none());
    assert_eq!(
        with_display.delicacy_digest(),
        without_display.delicacy_digest(),
        "the two tables must differ ONLY in the display field"
    );

    let identity = [up([0.0, 0.0, 1.0])];
    let scored = score(&with_display, &identity)[0];
    let baseline = score(&without_display, &identity)[0];

    // Every face this pose turns down is on the invisible glue face.
    let display = with_display.display();
    let downward_display_max = disc
        .tris()
        .iter()
        .enumerate()
        .filter(|(face, _)| {
            // Re-derive the downward mask here rather than reading the table's
            // own normals: the assertion is about which faces are down under
            // u = +Z, and a test that borrowed the table's arithmetic could
            // agree with it for the wrong reason.
            let verts = disc.verts();
            let triangle = disc.tris()[*face];
            let a = verts[triangle[0] as usize];
            let b = verts[triangle[1] as usize];
            let c = verts[triangle[2] as usize];
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let cross_z = e1[0] * e2[1] - e1[1] * e2[0];
            let length = ((e1[1] * e2[2] - e1[2] * e2[1]).powi(2)
                + (e1[2] * e2[0] - e1[0] * e2[2]).powi(2)
                + cross_z.powi(2))
            .sqrt();
            length > 0.0 && cross_z / length < -std::f64::consts::FRAC_1_SQRT_2
        })
        .map(|(face, _)| display[face])
        .fold(0.0_f64, f64::max);
    assert_eq!(
        downward_display_max, 0.0,
        "the identity pose turns down a face with D = {downward_display_max}; this fixture was \
         chosen because it does not, so the bitwise claim below would be vacuous"
    );

    assert!(
        scored.fragility_raw_mm2 > 0.0,
        "the identity pose turned down no fragile area; every bitwise equality below would then \
         compare zero with zero and prove nothing"
    );
    assert_eq!(scored.display_raw_mm2, 0.0);
    assert_eq!(scored.fragility_raw_mm2, baseline.fragility_raw_mm2);
    assert_eq!(scored.scar_raw_mm2, baseline.scar_raw_mm2);
    assert_eq!(scored.scar_term, baseline.scar_term);
    assert_eq!(scored.support_area_cm2, baseline.support_area_cm2);
    assert_eq!(scored.height_mm, baseline.height_mm);
    assert_eq!(scored.peak_mm2, baseline.peak_mm2);
    assert_eq!(scored.score, baseline.score);
}

/// **The published summary describes the field it names, and nothing else.**
///
/// The owner's ruling is that the map is published so it can be INSPECTED
/// before it steers anything, which is only true if the summary is bound to
/// the measurement. It carries the field's digest for exactly that reason.
#[test]
fn the_published_summary_is_bound_to_the_field_it_summarises() {
    let plate = two_sided_plate();
    let table = OrientFaceTable::build(&plate, "test-lod").expect("scoring table");
    let summary = table.display_summary().expect("a built field has a summary");
    assert_eq!(Some(summary.digest.as_str()), table.display_digest());
    assert_eq!(summary.faces, plate.tris().len());
    assert!(summary.total_area_mm2 > 0.0);
    let mass: f64 = summary.area_weighted_histogram.iter().sum();
    assert!(
        (mass - 1.0).abs() < 1e-9,
        "the area-weighted histogram holds {mass} of the area, not all of it"
    );
    assert!(
        summary.authored_up_area_weighted_mean > summary.authored_down_area_weighted_mean,
        "the summary reports authored-up D {:.6} at or below authored-down D {:.6} on a plate \
         whose two faces carry the same texture",
        summary.authored_up_area_weighted_mean,
        summary.authored_down_area_weighted_mean,
    );
    assert_eq!(summary.top_faces.len(), 8);
    assert!(summary.top_faces[0].display >= summary.top_faces[7].display);
    // A scar-free table builds no field at all, so there is nothing to
    // summarise — a stated absence, never a summary of zeros.
    let scar_free = OrientFaceTable::without_delicacy(&plate, "test-lod").expect("scar-free table");
    assert!(scar_free.display_summary().is_none());
}
