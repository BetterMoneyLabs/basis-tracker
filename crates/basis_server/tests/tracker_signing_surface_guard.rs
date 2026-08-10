#[test]
fn tracker_publisher_has_no_node_signer_or_configurable_change_surface() {
    let updater = include_str!("../src/tracker_box_updater.rs");

    assert!(updater.contains("Wallet::from_secrets"));
    assert!(updater.contains("sign_transaction(signing_context, state_context, None)"));
    assert!(updater.contains("/transactions"));
    assert!(!updater.contains("/wallet/transaction/sign"));
    assert!(!updater.contains("\"inputsRaw\""));
    assert!(!updater.contains("\"secrets\""));
    assert!(!updater.contains("change_address"));
}
