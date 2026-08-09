#[test]
fn tui_has_no_legacy_redemption_or_transaction_navigation() {
    let app = include_str!("../src/app.rs");
    let ui = include_str!("../src/ui.rs");

    for needle in ["Transactions", "RedeemNote", "GenerateTransaction"] {
        assert!(
            !app.contains(needle),
            "legacy Screen variant remains: {needle}"
        );
        assert!(
            !ui.contains(needle),
            "legacy TUI navigation remains: {needle}"
        );
    }

    for label in ["Redeem Note", "Generate Redemption Transaction"] {
        assert!(
            !ui.contains(label),
            "legacy TUI menu label remains: {label}"
        );
    }
}
