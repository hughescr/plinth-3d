use plinth_core::{Measured, PlateId};

fn main() {
    let _ = Measured::new(
        "unsupported",
        PlateId::from("plate"),
        None,
        "2026-07-31",
        "strings have no numeric or categorical measurement policy",
    );
}
