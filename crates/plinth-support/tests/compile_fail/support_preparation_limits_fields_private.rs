use plinth_support::SupportPreparationLimits;

fn main() {
    let _ = SupportPreparationLimits {
        max_grid_bytes: 0,
        ..SupportPreparationLimits::production().unwrap()
    };
}
