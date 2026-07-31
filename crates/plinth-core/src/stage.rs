use crate::StageId;

/// A pure typed pipeline step. Failures are reported as the output contract of
/// the owning stage, rather than erased into a bare boolean.
pub trait Stage {
    type Input;
    type Output;
    type Context: ?Sized;

    const ID: StageId;

    // The report is deliberately the concrete, sole failure contract shared by
    // every stage. Boxing it here would weaken the normative Stage signature.
    #[allow(clippy::result_large_err)]
    fn run(
        &self,
        input: Self::Input,
        ctx: &Self::Context,
    ) -> Result<Self::Output, crate::DefectReport>;
}
