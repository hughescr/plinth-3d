use plinth_support::FinalSupportAuthority;

fn dereference(authority: FinalSupportAuthority<'_>) {
    let _ = *authority;
}

fn main() {}
