//! The one integration-test binary of this crate (design section 4.3).

#[test]
fn cli_chain_is_emit_chain_plus_cli_canary() {
    assert_eq!(
        plinth_cli::canary_chain(),
        plinth_emit::canary_chain().wrapping_add(plinth_cli::loop_canary())
    );
}

#[test]
fn chitubox_chain_is_own_canary() {
    assert_eq!(
        plinth_chitubox::canary_chain(),
        plinth_chitubox::loop_canary()
    );
}
