use plinth_core::{AcceptSafe, ProvenanceRef};

#[derive(Clone, Copy)]
struct LocalValue(f64);

impl AcceptSafe<f64> for LocalValue {
    fn get(&self) -> f64 {
        self.0
    }

    fn provenance(&self) -> ProvenanceRef {
        ProvenanceRef::Derived {
            from: vec!["local".into()],
            note: "downstream implementation".into(),
        }
    }
}

fn main() {}
