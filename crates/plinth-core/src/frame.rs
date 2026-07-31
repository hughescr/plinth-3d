use std::marker::PhantomData;

/// A point-set frame marker.
pub trait Frame: sealed::Sealed {}

mod sealed {
    pub trait Sealed {}
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Source;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Centered;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Plated;

impl sealed::Sealed for Source {}
impl sealed::Sealed for Centered {}
impl sealed::Sealed for Plated {}
impl Frame for Source {}
impl Frame for Centered {}
impl Frame for Plated {}

/// Data explicitly tagged with its coordinate frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Framed<T, F: Frame> {
    value: T,
    frame: PhantomData<F>,
}

impl<T, F: Frame> Framed<T, F> {
    #[must_use]
    pub fn new(value: T) -> Self {
        Self {
            value,
            frame: PhantomData,
        }
    }

    #[must_use]
    pub fn into_inner(self) -> T {
        self.value
    }

    #[must_use]
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Framed<U, F> {
        Framed::new(f(self.value))
    }
}

impl<T, F: Frame> AsRef<T> for Framed<T, F> {
    fn as_ref(&self) -> &T {
        &self.value
    }
}
