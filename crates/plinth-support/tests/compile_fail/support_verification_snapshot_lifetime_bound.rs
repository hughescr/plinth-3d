use plinth_support::SupportVerificationSnapshot;

fn erase_lifetime(
    snapshot: SupportVerificationSnapshot<'_>,
) -> SupportVerificationSnapshot<'static> {
    snapshot
}

fn main() {}
