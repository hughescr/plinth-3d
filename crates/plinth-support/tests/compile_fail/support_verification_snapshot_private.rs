use plinth_support::SupportVerificationSnapshot;

fn alter(snapshot: &mut SupportVerificationSnapshot<'_>) {
    snapshot.snapshot_digest = 0;
}

fn main() {}
