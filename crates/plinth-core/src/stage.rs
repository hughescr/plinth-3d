use crate::StageId;

/// A pure typed pipeline step. Failures are reported as the output contract of
/// the owning stage, rather than erased into a bare boolean.
pub trait Stage {
    type Input;
    type Output;

    const ID: StageId;

    fn run(&self, input: Self::Input) -> Self::Output;
}
