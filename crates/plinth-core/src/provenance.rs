use crate::PlateId;
use serde::{Deserialize, Serialize};

/// A compact provenance description suitable for a run manifest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProvenanceRef {
    Measured { plate: PlateId, note: String },
    Derived { from: Vec<String>, note: String },
}

/// Values that are admissible on an acceptance path.
pub trait AcceptSafe<T: Copy> {
    fn get(&self) -> T;
    fn provenance(&self) -> ProvenanceRef;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Measured<T> {
    value: T,
    pub plate: PlateId,
    /// `None` is intentionally emitted as JSON `null`, never omitted.
    pub err: Option<T>,
    pub note: String,
}

impl<T> Measured<T> {
    #[must_use]
    pub fn new(value: T, plate: PlateId, err: Option<T>, note: impl Into<String>) -> Self {
        Self {
            value,
            plate,
            err,
            note: note.into(),
        }
    }

    #[must_use]
    pub fn value(&self) -> &T {
        &self.value
    }
}

impl<T: Copy> AcceptSafe<T> for Measured<T> {
    fn get(&self) -> T {
        self.value
    }

    fn provenance(&self) -> ProvenanceRef {
        ProvenanceRef::Measured {
            plate: self.plate.clone(),
            note: self.note.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Derived<T> {
    value: T,
    pub from: Vec<String>,
    pub note: String,
}

impl<T> Derived<T> {
    #[must_use]
    pub fn new(
        value: T,
        from: impl IntoIterator<Item = impl Into<String>>,
        note: impl Into<String>,
    ) -> Self {
        Self {
            value,
            from: from.into_iter().map(Into::into).collect(),
            note: note.into(),
        }
    }

    #[must_use]
    pub fn value(&self) -> &T {
        &self.value
    }
}

impl<T: Copy> AcceptSafe<T> for Derived<T> {
    fn get(&self) -> T {
        self.value
    }

    fn provenance(&self) -> ProvenanceRef {
        ProvenanceRef::Derived {
            from: self.from.clone(),
            note: self.note.clone(),
        }
    }
}

/// A value useful for diagnostics or search, but forbidden on acceptance paths.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Placeholder<T> {
    value: T,
    pub note: String,
}

impl<T> Placeholder<T> {
    #[must_use]
    pub fn new(value: T, note: impl Into<String>) -> Self {
        Self {
            value,
            note: note.into(),
        }
    }

    #[must_use]
    pub fn diagnostic(&self) -> &T {
        &self.value
    }
}

/// An explicitly unresolved calibration range.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pending<T> {
    pub conservative: T,
    pub optimistic: T,
    pub resolves_on: PlateId,
    pub question: String,
}

impl<T: Copy> Pending<T> {
    #[must_use]
    pub fn new(
        conservative: T,
        optimistic: T,
        resolves_on: PlateId,
        question: impl Into<String>,
    ) -> Self {
        Self {
            conservative,
            optimistic,
            resolves_on,
            question: question.into(),
        }
    }

    /// Resolving uses the conservative branch and returns an acceptance-safe value.
    #[must_use]
    pub fn resolve(&self) -> Derived<T> {
        Derived::new(
            self.conservative,
            [format!("pending plate {}", self.resolves_on)],
            format!("resolved conservatively: {}", self.question),
        )
    }

    /// This is for calibration comparison only; it is still explicitly derived.
    #[must_use]
    pub fn optimistic(&self) -> Derived<T> {
        Derived::new(
            self.optimistic,
            [format!("pending plate {}", self.resolves_on)],
            format!("optimistic calibration branch: {}", self.question),
        )
    }
}
