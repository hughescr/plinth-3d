//! **A plate root's disc is the root the file draws** — the congestion
//! census, and every clearance decision that reads the same discs, see the
//! root the lowering actually draws (review-fixes, 2026-09-29).
//!
//! # The defect
//!
//! `congestion::root_discs` sized every root disc from the LEDGER root,
//! `radius_mm[0]`. The lowering draws the DRAWN root: the ledger raised by the
//! junction clause wherever a thicker branch joins (in practice a brace, sized
//! by its own slenderness minimum, which the ledger never hears of). So a
//! 0.05 mm plate arm 0.50 mm from a 0.20 mm root reads, at the registered flare
//! factor 1.6, as a 0.40 mm flare sum that clears; once a 0.20 mm brace joins
//! it, the file draws its root at 0.20 mm, a 0.64 mm flare sum, and the two
//! discs lap by 0.14 mm while the census still says 0.40 mm and no overlap.
//!
//! # The rule
//!
//! A root disc is sized from the same drawn profile the lowering draws, by the
//! lowering's own root rule (`plinth_support::drawn_root_radius_mm`), times
//! the crowding factor `FLARE_R_FACTOR`.
//!
//! **Which ring of the plate root (2026-09-30).** Until then the disc was also
//! the emitted flare's FOOT, and this test read the feet. Since the owner's
//! ChituBox geometry the lowering stands every plate root on a FIXED boss
//! (1.1 mm foot, grown only for a wider root) and the census keeps its
//! crowding disc (`SweepPolicy::flare_r_factor` says why). The drawn root is
//! the boss's TOP ring, the `RootFlare` record's start radius, so this test
//! now reads that ring, times the factor.
//!
//! # How it is graded
//!
//! The reviewer's case, built as a strut set on a real sweep plan's grid and
//! frame, is lowered by the production adapter (`to_primitives`). The census
//! (`congestion::measure`) must count exactly the emitted root bosses, their
//! total analytic crowding-disc area, and the overlapping pairs those discs
//! form — read off the records' own start points and radii. Anti-vacuity: the
//! emitted roots' discs must lap (one pair), or the fixture grades nothing.
//!
//! Sabotage: size the discs from `radius_mm[0]` again and the test goes red.

use plinth_core::{m0_constants, Finite, LayerPlan, PositiveFinite};
use plinth_mesh::{make_lonely_peak, Mesh};
use plinth_support::{BuildFrame, SupportSegmentKind};
use plinth_sweep::{
    congestion, to_primitives, up_sweep::ColumnHistory, Landing, SeatFace, StrutKind, SweepPolicy,
    SweepStrut,
};

const LIFT_MM: f64 = 5.0;
const MAX_PRIMITIVES: usize = 100_000;
const MAX_PRIMITIVE_BYTES: usize = 1 << 26;

fn layer_height_mm() -> f64 {
    *m0_constants()
        .expect("registry")
        .evidence()
        .layer_height_default_mm()
        .value()
}

fn z_span(mesh: &Mesh) -> (f64, f64) {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for vertex in mesh.verts() {
        min = min.min(vertex[2]);
        max = max.max(vertex[2]);
    }
    (min, max)
}

#[expect(clippy::too_many_arguments, reason = "one plain record per member")]
fn member(
    id: u32,
    cell: usize,
    centre: [f64; 2],
    kind: StrutKind,
    k_bottom: usize,
    k_top: usize,
    landing: Landing,
    radius: f64,
) -> SweepStrut {
    let slots = k_top - k_bottom;
    SweepStrut {
        id,
        cell,
        center_mm: centre,
        kind,
        min_radius_mm: if kind == StrutKind::Brace {
            radius
        } else {
            0.0
        },
        thickened_mm: Vec::new(),
        seat_on_strut: None,
        k_top,
        k_bottom,
        landing,
        interior: false,
        radius_mm: vec![radius; slots],
        carried_mm2: vec![0.0; slots],
        covered_demand: Vec::new(),
        inserted_by_load: kind != StrutKind::Arm,
        path: Vec::new(),
        straight_fallback: None,
        fallback_path: Vec::new(),
        seat_face: SeatFace::default(),
        seat_normal: [0.0; 3],
    }
}

#[test]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "cell offsets of a few pitches and layer counts of a small fixture"
)]
fn the_census_counts_the_root_flares_the_file_draws_under_a_braced_post() {
    // A real plan for its grid, frame and report; its strut set is replaced.
    let mesh = make_lonely_peak().expect("the lonely peak fixture");
    let (min_z, max_z) = z_span(&mesh);
    let plate_z = min_z - LIFT_MM;
    let height = layer_height_mm();
    let count = (((max_z - plate_z) / height).ceil() as usize).saturating_add(1);
    let layers = LayerPlan::new(plate_z, vec![height; count]).expect("a uniform layer plan");
    let policy = SweepPolicy::from_registry().expect("the registry supplies every sweep number");
    let mut plan = plinth_sweep::run_sweep(&mesh, &layers, &policy)
        .expect("the sweep emits and warns; it never refuses");
    let grid = plan.grid;
    let flare = policy.flare_r_factor;

    // P (0.05 mm) and Q (0.20 mm) roots about 0.50 mm apart: their LEDGER
    // flares clear, P's drawn root after a 0.20 mm brace does not.
    let pitch = grid.pitch_mm();
    let gap_cells = (0.5 / pitch).round() as usize;
    let gap_mm = gap_cells as f64 * pitch;
    assert!(
        (0.05 + 0.20) * flare < gap_mm && 2.0 * 0.20 * flare > gap_mm,
        "a {gap_mm} mm gap on this grid does not separate the ledger case from the drawn one"
    );
    let row = grid.rows() / 2;
    let col = grid.cols() / 4;
    let at = |col: usize| {
        let cell = grid.index(row, col);
        (cell, [grid.center_x_mm(col), grid.center_y_mm(row)])
    };
    let k_top = (4.0 / height).round() as usize;
    let (p_cell, p_centre) = at(col);
    let (q_cell, q_centre) = at(col + gap_cells);
    let (r_cell, r_centre) = at(col.saturating_sub(4 * gap_cells));
    let plate_arm = |id: u32, cell: usize, centre: [f64; 2], radius: f64| {
        member(
            id,
            cell,
            centre,
            StrutKind::Arm,
            0,
            k_top,
            Landing::Plate,
            radius,
        )
    };
    let p = plate_arm(0, p_cell, p_centre, 0.05);
    let q = plate_arm(1, q_cell, q_centre, 0.20);
    let r = plate_arm(2, r_cell, r_centre, 0.20);
    // The brace: tops out on P, lands on R lower down.
    let (k_hi, k_lo) = (k_top / 2, k_top / 4);
    let mut brace = member(
        3,
        p_cell,
        p_centre,
        StrutKind::Brace,
        k_lo,
        k_hi,
        Landing::Strut { id: 2, layer: k_lo },
        0.20,
    );
    brace.seat_on_strut = Some(0);
    plan.struts = vec![p, q, r, brace];

    let history = ColumnHistory::new(&grid);
    let census = congestion::measure(&grid, &history, &plan.struts, flare).all;

    let frame = BuildFrame::new(
        Finite::new(plate_z).expect("a finite plate"),
        PositiveFinite::new(LIFT_MM).expect("a positive lift"),
    );
    let emission = to_primitives(&plan, frame, &layers, MAX_PRIMITIVES, MAX_PRIMITIVE_BYTES)
        .expect("the lowering refuses only about the request");
    let feet: Vec<([f64; 3], f64)> = emission
        .primitives
        .iter()
        .filter(|primitive| primitive.segment_kind == SupportSegmentKind::RootFlare)
        .map(|boss| {
            (
                boss.start_mm.map(Finite::get),
                boss.start_radius_mm.get() * flare,
            )
        })
        .collect();
    let emitted_area: f64 = feet
        .iter()
        .map(|(_, radius)| core::f64::consts::PI * radius * radius)
        .sum();
    let mut emitted_laps = 0;
    for (i, (a, ra)) in feet.iter().enumerate() {
        for (b, rb) in &feet[i + 1..] {
            let d = (a[0] - b[0]).hypot(a[1] - b[1]);
            emitted_laps += usize::from(d < ra + rb);
        }
    }
    println!(
        "root flares emitted {} (area {emitted_area:.6} mm2, {emitted_laps} lapping pairs); \
         census {} roots (area {:.6} mm2, {} lapping pairs)",
        feet.len(),
        census.roots,
        census.analytic_disc_area_mm2,
        census.overlap_pairs
    );
    assert_eq!(feet.len(), 3, "the three plate arms each draw a root boss");
    assert!(
        emitted_laps > 0,
        "the emitted roots do not lap, so the fixture grades nothing"
    );
    assert_eq!(census.roots, feet.len());
    assert!(
        (census.analytic_disc_area_mm2 - emitted_area).abs() <= 1e-9 * emitted_area,
        "the census lays {:.6} mm2 of root disc on the plate; the file draws {emitted_area:.6}",
        census.analytic_disc_area_mm2
    );
    assert_eq!(
        census.overlap_pairs, emitted_laps,
        "the census and the file disagree about which feet lap"
    );
}
