use plinth_support::FinalSupportAuthority;

fn erase_lifetime(authority: FinalSupportAuthority<'_>) -> FinalSupportAuthority<'static> {
    authority
}

fn main() {}
