use basis_store::{
    ExtendedReserveInfo, ReserveTracker,
};
use hex;

#[tokio::test]
async fn test_token_reserve_creation_and_parsing() {
    let tracker = ReserveTracker::new();
    
    // Create a reserve with Token collateral
    let box_id = b"token_reserve_box_1";
    let owner_pubkey = [0x02u8; 33];
    let erg_value = 1_000_000u64; // Min ERG
    let token_id = b"test_token_id_12345678901234567890123456789012";
    let token_amount = 5000u64;

    let reserve_info = ExtendedReserveInfo::new(
        box_id,
        &owner_pubkey,
        erg_value,
        None,
        1000,
        Some(token_id),
        Some(token_amount),
    );
    
    // 1. Verify fields are set correctly
    assert_eq!(reserve_info.base_info.token_id, Some(hex::encode(token_id)));
    assert_eq!(reserve_info.base_info.token_amount, Some(token_amount));

    // 2. Add to tracker
    let result = tracker.update_reserve(reserve_info.clone());
    assert!(result.is_ok(), "Token reserve should be added successfully");
    
    // 3. Retrieve and verify
    let retrieved = tracker.get_reserve(&hex::encode(box_id)).unwrap();
    assert_eq!(retrieved.base_info.token_amount, Some(token_amount));
    
    // 4. Test logic with token reserves (collateralization ratio currently ignores token value, fix later)
    // For now we just verify it doesn't crash calculations
    let ratio = retrieved.collateralization_ratio();
    assert_eq!(ratio, f64::INFINITY);
}
