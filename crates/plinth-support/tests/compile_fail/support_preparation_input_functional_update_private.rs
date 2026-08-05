use plinth_core::LayerPlan;
use plinth_support::{SupportPreparationInput, SupportPreparationLimits};

fn main() {
    let layers = LayerPlan::new(0.0, vec![0.05]).unwrap();
    let base = SupportPreparationLimits::production()
        .unwrap()
        .input(&layers, &[], &[]);
    let _ = SupportPreparationInput {
        max_grid_bytes: 0,
        ..base
    };
}
