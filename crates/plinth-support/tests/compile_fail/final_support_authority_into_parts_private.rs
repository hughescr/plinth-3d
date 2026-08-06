use plinth_support::FinalSupportAuthority;

fn extract(authority: FinalSupportAuthority<'_>) {
    let _ = authority.into_parts();
}

fn main() {}
