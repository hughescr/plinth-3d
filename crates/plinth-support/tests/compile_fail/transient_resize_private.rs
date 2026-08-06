use plinth_core::Finite;
use plinth_support::forest::Forest;

fn main() {
    let forest = Forest::new();
    let _ = plinth_support::transient::resize_forest_current_parity(
        &forest,
        Finite::new(0.0).unwrap(),
    );
}
