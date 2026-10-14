//! **R6 + D4: brace bands counted through a frame in which every band can
//! sway** (Craig ruling 2026-10-03, interim pending the slenderness coupon;
//! `field-2026-09-28/bracing-mechanics.md` revision 2, §3 and rule R6).
//!
//! The study's headline: "a two-post pair needs ~2 bands at 2x its limit, 3
//! at 2.5x, 5-6 at 3x (the engine placed 1, 2, 2)". The engine credited
//! every band as a fixed point, so it counted `⌈L / limit⌉ − 1`.
//!
//! The fixture is the study's own thin pair: two plate posts of radius
//! 0.15 mm (the Cylinder and Sphere median), 2.2 mm apart, `S = 40`, run at
//! 2, 2.5 and 3 times their limit. Only the brace rung fires (no merge, no
//! thickening, no mast, no resin window), so the count is the brace rule's
//! alone.
//!
//! **What grades it is not the lane's own count.** The bands the ladder
//! BUILT are read off the strut set (each cross's two layers and its
//! diagonal radius) and put into the two-post panel
//! ([`plinth_sweep::frame_bands::panel_sway`]), whose numbers are held cell
//! by cell to the study's `bands2.ts` on the study's own fixtures
//! (`frame_bands::tests::the_panel_reproduces_the_study_band_table`). The
//! grade is the run's top sway under drag with the bands as built: within one
//! pixel, or not.
//!
//! | test | asserts |
//! |---|---|
//! | `the_pair_gets_the_bands_its_frame_needs` | with R6 on, at λ 2, 2.5 and 3 the built bands hold the run within one pixel in the frame, with at least the study's counts (2, 3, 5) |
//! | `sabotage_fixed_point_bands_leave_the_frame_swaying` | the drill (R6 off, develop's ladder): 1, 2, 2 bands, and the frame sways past one pixel at every λ (the red) |
//! | `the_fast_count_builds_the_bands_the_full_sway_counts_on_the_pair_fixtures` | the band-speed lane's in-plane count gives the same bands (and the same ceiling, most bands and sway to the last bit) as the full two-solve count it replaced, at λ 2, 2.5, 3 and 4 |
//!
//! Red on develop: develop has no `frame_bands` knob; its ladder IS the
//! sabotage arm here (`without_frame_bands`, which restores it exactly), and
//! that arm's grade is the failing one. Lane report:
//! `field-2026-09-28/lanes/brace-frame-bands.md`.

#![allow(
    clippy::cast_precision_loss,
    reason = "layer indices of a 20 mm fixture are exact in f64"
)]

use plinth_sweep::drain::DrainKeepOut;
use plinth_sweep::frame_bands::{panel_sway, Panel, PanelBand};
use plinth_sweep::grid::{lay_out_at, Grid};
use plinth_sweep::lean::LeanLaw;
use plinth_sweep::slenderness::{apply_ties, SlendernessContext, SlendernessLaw, SlendernessOutcome};
use plinth_sweep::tip_landing::SeatFaceRule;
use plinth_sweep::up_sweep::ColumnHistory;
use plinth_sweep::{Landing, SeatFace, StrutKind, SweepStrut};

const H: f64 = 0.05;
const S: f64 = 40.0;
const RADIUS: f64 = 0.15;
/// 2.2 mm at the 0.05 mm pitch.
const GAP_CELLS: usize = 44;

fn post(grid: &Grid, id: u32, col: usize, layers: usize) -> SweepStrut {
    let row = 100;
    SweepStrut {
        id,
        cell: grid.index(row, col),
        center_mm: [grid.center_x_mm(col), grid.center_y_mm(row)],
        kind: StrutKind::Arm,
        min_radius_mm: 0.0,
        thickened_mm: Vec::new(),
        seat_on_strut: None,
        k_top: layers,
        k_bottom: 0,
        landing: Landing::Plate,
        interior: false,
        radius_mm: vec![RADIUS; layers],
        carried_mm2: vec![0.0; layers],
        covered_demand: Vec::new(), held_birth_cells: Vec::new(), twin_host: false,
        inserted_by_load: false,
        path: Vec::new(),
        seat_face: SeatFace::default(),
        seat_normal: [0.0; 3],
        straight_fallback: None,
        fallback_path: Vec::new(),
        head: None,
    }
}

/// What one run of the ladder built on the pair: its crosses, and the run's
/// top sway in the panel with exactly those bands.
#[derive(Debug)]
struct Built {
    crosses: usize,
    drag_over_pixel: f64,
    outcome: SlendernessOutcome,
}

fn tie_the_pair(lambda: f64, frame_bands: bool) -> Built {
    let grid = lay_out_at(0.0, 0.0, H, 200, 200);
    let history = ColumnHistory::new(&grid);
    let keep_out = DrainKeepOut::none();
    let lean = LeanLaw::new(H, 45.0, 25.0);
    let law = SlendernessLaw::new(S, 0.05, 1.0, 32.5);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a positive run length in layers, rounded"
    )]
    let layers = (lambda * S * RADIUS / H).round() as usize;
    let context = SlendernessContext {
        grid: &grid,
        history: &history,
        keep_out: &keep_out,
        lean: &lean,
        law: &law,
        layer_height_mm: H,
        layers: layers + 100,
        flare_factor: 1.6,
        mast_root_clearance: true,
        final_clearance_mm: 0.3,
        member_clearance: true,
        model_surface: None,
        plate_z_mm: 0.0,
        shaft_landings: true,
        merge: false,
        brace: true,
        thicken: false,
        mast: false,
        max_struts: 1_000,
        x_allowed: 12.0,
        seat_faces: SeatFaceRule::from_overhang_band(45.0, 25.0),
        load_step_taper: plinth_sweep::adapter::LoadStepTaper::of(&lean),
        brace_cap: true,
        brace_window: false,
        frame_bands,
        frame_ledger: true,
        model_tie: false,
    };
    let mut struts = vec![post(&grid, 0, 60, layers), post(&grid, 1, 60 + GAP_CELLS, layers)];
    let mut outcome = SlendernessOutcome::default();
    apply_ties(&context, &mut struts, &mut outcome);
    // The bands as BUILT: every diagonal seated on post 0 is one cross.
    let bands: Vec<PanelBand> = struts
        .iter()
        .filter(|strut| strut.kind == StrutKind::Brace && strut.seat_on_strut == Some(0))
        .map(|brace| PanelBand {
            bottom_mm: brace.k_bottom as f64 * H,
            top_mm: brace.k_top as f64 * H,
            brace_radius_mm: brace.min_radius_mm,
        })
        .collect();
    let gap = GAP_CELLS as f64 * H;
    let sway = panel_sway(
        &Panel {
            post_radius_mm: RADIUS,
            partner_radius_mm: RADIUS,
            gap_mm: gap,
            height_mm: layers as f64 * H,
            bands: bands.clone(),
        },
        S,
    )
    .expect("the built bands fit the run");
    Built {
        crosses: bands.len(),
        drag_over_pixel: sway.drag_over_pixel,
        outcome,
    }
}

#[test]
fn the_pair_gets_the_bands_its_frame_needs() {
    for (lambda, study) in [(2.0, 2), (2.5, 3), (3.0, 5)] {
        let built = tie_the_pair(lambda, true);
        println!(
            "R6 on, λ {lambda}: {} crosses, frame sway {:.3} px ({} sets, {} beyond the \
             fixed-point count, {} moved, {} respaced)",
            built.crosses,
            built.drag_over_pixel,
            built.outcome.frame_band_sets,
            built.outcome.frame_bands_beyond_fixed_point,
            built.outcome.frame_bands_moved_from_their_layer,
            built.outcome.frame_sets_respaced,
        );
        assert!(
            built.drag_over_pixel <= 1.0 + 1e-9,
            "λ {lambda}: the built bands must hold the run in the frame: {built:?}"
        );
        assert!(
            built.crosses >= study,
            "λ {lambda}: at least the study's {study} bands: {built:?}"
        );
        assert!(built.outcome.frame_band_sets >= 1, "path hit: {built:?}");
        assert!(
            built.outcome.frame_bands_beyond_fixed_point >= 1,
            "path hit, the frame added bands: {built:?}"
        );
    }
}

#[test]
fn sabotage_fixed_point_bands_leave_the_frame_swaying() {
    for (lambda, fixed_point) in [(2.0, 1), (2.5, 2), (3.0, 2)] {
        let built = tie_the_pair(lambda, false);
        println!(
            "R6 off, λ {lambda}: {} crosses, frame sway {:.3} px",
            built.crosses, built.drag_over_pixel
        );
        assert_eq!(built.crosses, fixed_point, "λ {lambda}: {built:?}");
        assert!(
            built.drag_over_pixel > 1.0,
            "λ {lambda}: fixed-point bands leave the frame over one pixel: {built:?}"
        );
        assert_eq!(built.outcome.frame_band_sets, 0);
    }
}

/// **The fast count builds the bands the full sway counts** (band-speed lane,
/// 2026-10-04): on this file's own thin pair at 2, 2.5, 3 and 4 times its
/// limit, `count_bands` (one in-plane solve a count) and the loop as it stood
/// before the lane (`count_bands_by_full_sway`, two full solves a count) say
/// the same thing, to the last bit of the sway: the fewest bands that hold,
/// or the same ceiling with the same most bands. The bands are laid as the
/// study lays them, `rise` deep, at the depth the overhang law gives this gap.
/// The headline counts (2, 3 and 5 at 2, 2.5 and 3 times) are read off the
/// fast one.
#[test]
fn the_fast_count_builds_the_bands_the_full_sway_counts_on_the_pair_fixtures() {
    use plinth_sweep::frame_bands::{count_bands, count_bands_by_full_sway, BandCount};
    let gap = GAP_CELLS as f64 * H;
    let lean = LeanLaw::new(H, 45.0, 25.0);
    let rise = lean.layers_to_reach(gap) as f64 * H;
    let brace = 0.05_f64.max(gap.hypot(rise) / S);
    for (lambda, study) in [(2.0, Some(2)), (2.5, Some(3)), (3.0, Some(5)), (4.0, None)] {
        let height = (lambda * S * RADIUS / H).round() * H;
        let panel = Panel {
            post_radius_mm: RADIUS,
            partner_radius_mm: RADIUS,
            gap_mm: gap,
            height_mm: height,
            bands: Vec::new(),
        };
        let layout = |n: usize| {
            (n <= 12).then(|| {
                (1..=n)
                    .map(|i| {
                        let centre = height * i as f64 / (n + 1) as f64;
                        PanelBand {
                            bottom_mm: centre - 0.5 * rise,
                            top_mm: centre + 0.5 * rise,
                            brace_radius_mm: brace,
                        }
                    })
                    .collect()
            })
        };
        let fast = count_bands(&panel, S, 1, &layout);
        let full = count_bands_by_full_sway(&panel, S, 1, &layout);
        println!("λ {lambda}: {fast:?}");
        assert_eq!(fast, full, "λ {lambda}");
        match (fast, study) {
            (BandCount::Holds { bands, .. }, Some(want)) => {
                assert!(bands >= want, "λ {lambda}: {bands} bands, the study's {want}");
            }
            (BandCount::PastCeiling { .. }, None) => {}
            _ => panic!("λ {lambda}: {fast:?} against the study's {study:?}"),
        }
    }
}
