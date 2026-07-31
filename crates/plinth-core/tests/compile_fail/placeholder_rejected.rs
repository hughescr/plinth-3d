use plinth_core::{AcceptSafe, Placeholder};

fn acceptance_input(value: &impl AcceptSafe<f64>) -> f64 {
    value.get()
}

fn main() {
    let guess = Placeholder::new(0.05_f64, "not measured");
    let _ = acceptance_input(&guess);
}
