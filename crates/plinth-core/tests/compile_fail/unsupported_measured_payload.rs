use plinth_core::{MeasuredPayload, ProvenanceError};

struct LocalPayload;

impl MeasuredPayload for LocalPayload {
    fn validate_measurement(
        _value: &Self,
        _err: Option<&Self>,
    ) -> Result<(), ProvenanceError> {
        Ok(())
    }
}

fn main() {}
