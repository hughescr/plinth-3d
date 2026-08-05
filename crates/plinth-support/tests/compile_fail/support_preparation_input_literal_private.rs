use plinth_core::LayerPlan;
use plinth_support::SupportPreparationInput;

fn main() {
    let layers = LayerPlan::new(0.0, vec![0.05]).unwrap();
    let _ = SupportPreparationInput {
        layers: &layers,
        delicacy_by_face: &[],
    };
}
