//! Print the production orientation search's COMMIT tier for one or more STL
//! files, term by term, so a pose decision can be read off its terms instead
//! of inferred from the winner.
//!
//! The chain is `plinth prepare`'s own order up to orientation
//! (`plinth-pipeline` `prepare_one` / `orient_model`): load with no weld,
//! `repair_mesh`, registered decimation, then the dense search with the
//! registered sweep request and `SweepDemand::production()`. The decimation
//! budget is built the way `tests/it/round_bases_ratchet.rs` builds it
//! (plinth-sweep cannot depend on plinth-pipeline, so the small body is
//! repeated, as there).
//!
//! `cargo run -p plinth-sweep --profile gate --example pose_ranking_probe -- <a.stl> [b.stl ...]`
//!
//! Added by the scar-fold-invariance lane (2026-10-01) to report every corpus
//! model's pose before and after the fragility fold was made tessellation
//! invariant, and to run the one-solid toed slab (exported from
//! `fix/toed-slab-fixture-2026-09-30` at `7328faf`) through the production
//! chain without landing that fixture.

use std::path::PathBuf;
use std::time::Instant;

use plinth_core::m0_constants;
use plinth_decimate::{BudgetConfigError, ProbeConfig};
use plinth_mesh::{load_binary_stl, WeldPolicy};
use plinth_orient::registered_sweep_request;
use plinth_orient::search::{search, SearchTier};
use plinth_sweep::SweepDemand;

fn registered_decimation_budget() -> plinth_decimate::DecimationBudget {
    let placeholders = m0_constants().expect("registry").placeholders();
    let probe = ProbeConfig::new(
        placeholders
            .decimation_probe_grid_pitch_mm()
            .diagnostic()
            .get(),
        usize::try_from(
            placeholders
                .decimation_probe_max_grid_bytes()
                .diagnostic()
                .get(),
        )
        .map_err(|_| BudgetConfigError::InvalidGridBytes { value: 0 })
        .expect("registered grid byte count fits a usize"),
        placeholders
            .decimation_probe_nearest_upper_bound_mm()
            .diagnostic()
            .get(),
    );
    let budget = plinth_decimate::DecimationBudget::ratified_default(probe)
        .expect("the ratified probe config builds a budget");
    budget
        .validate_for_execution()
        .expect("the registered budget is executable");
    budget
}

fn main() {
    let paths: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    assert!(!paths.is_empty(), "usage: pose_ranking_probe <a.stl> [b.stl ...]");
    for path in paths {
        let started = Instant::now();
        let raw = load_binary_stl(&path, WeldPolicy::None).expect("load the STL");
        let repaired = plinth_mesh::repair_mesh(&raw).into_mesh();
        let decimated = plinth_decimate::run(&repaired, registered_decimation_budget())
            .expect("decimation must run")
            .into_decimated_model()
            .expect("scale sanity must pass")
            .into_parts()
            .0;
        let request = registered_sweep_request().expect("the registered sweep request");
        let demand = SweepDemand::production();
        let outcome = search(&decimated, &request, Some(&demand))
            .expect("the dense orientation search must not refuse");
        let record = outcome.record();
        let commit = record
            .tiers
            .iter()
            .find(|tier| tier.tier == SearchTier::Commit)
            .expect("a commit tier");
        println!(
            "== {} : {} faces decimated, {:.1} s",
            path.display(),
            decimated.tris().len(),
            started.elapsed().as_secs_f64()
        );
        for entry in &commit.ranking {
            let up = entry.up_model_frame.map(|value| value.get());
            let terms = &entry.terms;
            println!(
                "rank {:>2} up ({:+.4},{:+.4},{:+.4}) tilt {:6.2} score {:.6} support {:.6} cm2 \
                 peel {:.6} height {:.3} scar_term {:.6} scar_raw {:.4} fragility {:.4} \
                 display {:.4} landing {:?} tip {:?} down_faces {}",
                entry.rank,
                up[0],
                up[1],
                up[2],
                up[2].clamp(-1.0, 1.0).acos().to_degrees(),
                terms.score.get(),
                terms.support_area_cm2.get(),
                terms.peel_term.get(),
                terms.height_mm.get(),
                terms.scar_term.get(),
                terms.scar_raw_mm2.get(),
                terms.fragility_raw_mm2.get(),
                terms.display_raw_mm2.get(),
                terms.landing_scar_raw_mm2.map(|value| value.get()),
                terms.tip_scar_raw_mm2.map(|value| value.get()),
                terms.down_facing_faces,
            );
        }
        let winner = record.winner.up_model_frame.map(|value| value.get());
        println!(
            "WINNER {} up ({:+.4},{:+.4},{:+.4}) tilt {:.2} score {:.6}",
            path.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned()),
            winner[0],
            winner[1],
            winner[2],
            winner[2].clamp(-1.0, 1.0).acos().to_degrees(),
            record.winner.terms.score.get(),
        );
    }
}
