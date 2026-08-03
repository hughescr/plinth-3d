use plinth_support::SupportVerificationSnapshot;

fn duplicate(snapshot: SupportVerificationSnapshot<'_>) {
    let _ = snapshot.clone();
}

fn main() {}
