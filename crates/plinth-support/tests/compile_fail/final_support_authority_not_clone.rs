use plinth_support::FinalSupportAuthority;

fn duplicate(authority: FinalSupportAuthority<'_>) {
    let _ = authority.clone();
}

fn main() {}
