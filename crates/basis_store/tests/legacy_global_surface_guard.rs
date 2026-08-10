#[test]
fn tracker_state_manager_has_no_legacy_global_proof_or_reserve_surface() {
    let source = include_str!("../src/lib.rs");
    let forbidden = [
        "reserve_avl_state",
        "pub struct NoteProof",
        "pub struct TrackerLookupProof",
        "pub struct ReserveLookupProof",
        "pub fn generate_proof(",
        "pub fn generate_tracker_lookup_proof(",
        "pub fn get_already_redeemed(",
        "pub fn generate_reserve_lookup_proof(",
        "pub fn generate_reserve_insert_proof(",
        "pub fn reserve_state_digest(",
        "pub fn update_already_redeemed(",
    ];

    for needle in forbidden {
        assert!(
            !source.contains(needle),
            "legacy global v1 surface remains in TrackerStateManager: {needle}"
        );
    }
}
